use std::{error::Error, fmt, marker::PhantomData};

use futures::{Sink, Stream};
use tokio::sync::mpsc::{self, Receiver, Sender};

use rmcp::{
    serve_client, serve_server,
    service::{
        RoleClient, RoleServer, RunningService, RxJsonRpcMessage, ServiceRole, TxJsonRpcMessage,
    },
    transport::IntoTransport,
    Service,
};

#[derive(Debug)]
pub struct InProcessTransportError(String);

impl fmt::Display for InProcessTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InProcess transport error: {}", self.0)
    }
}

impl Error for InProcessTransportError {}

pub struct ClientSendError(mpsc::error::SendError<TxJsonRpcMessage<RoleClient>>);
pub struct ServerSendError(mpsc::error::SendError<TxJsonRpcMessage<RoleServer>>);

impl From<ClientSendError> for InProcessTransportError {
    fn from(err: ClientSendError) -> Self {
        InProcessTransportError(format!("Failed to send client message: {}", err.0))
    }
}

impl From<ServerSendError> for InProcessTransportError {
    fn from(err: ServerSendError) -> Self {
        InProcessTransportError(format!("Failed to send server message: {}", err.0))
    }
}

impl From<InProcessTransportError> for std::io::Error {
    fn from(value: InProcessTransportError) -> Self {
        std::io::Error::other(value)
    }
}

impl From<std::io::Error> for InProcessTransportError {
    fn from(err: std::io::Error) -> Self {
        InProcessTransportError(format!("IO error: {}", err))
    }
}

/// Creates a pair of transports that can be used for client and server
/// running in the same process. This avoids network overhead entirely.
///
/// The `buffer_size` parameter controls how many messages can be buffered before
/// back-pressure is applied.
pub fn create_in_process_transport_pair(
    buffer_size: usize,
) -> (
    InProcessTransport<RoleClient>,
    InProcessTransport<RoleServer>,
) {
    // Client to Server channel (client sends, server receives)
    let (client_tx, server_rx) = mpsc::channel::<TxJsonRpcMessage<RoleClient>>(buffer_size);
    // Server to Client channel (server sends, client receives)
    let (server_tx, client_rx) = mpsc::channel::<TxJsonRpcMessage<RoleServer>>(buffer_size);

    let client_transport = InProcessTransport::<RoleClient> {
        tx: client_tx,
        rx: client_rx,
        _phantom: PhantomData,
    };

    let server_transport = InProcessTransport::<RoleServer> {
        tx: server_tx,
        rx: server_rx,
        _phantom: PhantomData,
    };

    (client_transport, server_transport)
}

/// Create a new in-process transport pair with matching client and server sides
pub fn create_transport_pair(
    buffer_size: usize,
) -> (
    InProcessTransport<RoleClient>,
    InProcessTransport<RoleServer>,
) {
    create_in_process_transport_pair(buffer_size)
}

/// A unified transport that can be used for both client and server sides
/// of in-process communication
pub struct InProcessTransport<R: ServiceRole> {
    tx: Sender<TxJsonRpcMessage<R>>,
    rx: Receiver<RxJsonRpcMessage<R>>,
    _phantom: PhantomData<R>,
}

impl<R: ServiceRole> InProcessTransport<R> {
    /// Split the transport into its sink and stream components
    pub fn split(self) -> (InProcessSink<R>, InProcessStream<R>) {
        let sink = InProcessSink {
            tx: self.tx,
            _phantom: PhantomData,
        };
        let stream = InProcessStream {
            rx: self.rx,
            _phantom: PhantomData,
        };
        (sink, stream)
    }

    /// Create a new transport pair with the given buffer size
    pub fn new(
        buffer_size: usize,
    ) -> (
        InProcessTransport<RoleClient>,
        InProcessTransport<RoleServer>,
    ) {
        create_in_process_transport_pair(buffer_size)
    }
}

/// Extension methods for InProcessTransport to simplify common operations
pub trait InProcessTransportExt<R: ServiceRole> {
    /// Create a new service using this transport
    fn serve<S>(
        self,
        service: S,
    ) -> impl std::future::Future<Output = Result<RunningService<R, S>, InProcessTransportError>> + Send
    where
        S: Service<R> + Send + 'static;
}

impl InProcessTransportExt<RoleClient> for InProcessTransport<RoleClient> {
    async fn serve<S>(
        self,
        service: S,
    ) -> Result<RunningService<RoleClient, S>, InProcessTransportError>
    where
        S: Service<RoleClient> + Send + 'static,
    {
        serve_client(service, self).await
    }
}

impl InProcessTransportExt<RoleServer> for InProcessTransport<RoleServer> {
    async fn serve<S>(
        self,
        service: S,
    ) -> Result<RunningService<RoleServer, S>, InProcessTransportError>
    where
        S: Service<RoleServer> + Send + 'static,
    {
        serve_server(service, self).await
    }
}

/// The sink component of the in-process transport
pub struct InProcessSink<R: ServiceRole> {
    tx: Sender<TxJsonRpcMessage<R>>,
    _phantom: PhantomData<R>,
}

pin_project_lite::pin_project! {
    /// The stream component of the in-process transport
    pub struct InProcessStream<R: ServiceRole> {
        #[pin]
        rx: Receiver<RxJsonRpcMessage<R>>,
        _phantom: PhantomData<R>,
    }
}

impl<R: ServiceRole> Sink<TxJsonRpcMessage<R>> for InProcessSink<R> {
    type Error = InProcessTransportError;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // MPSC Sender doesn't have poll_ready, we just check if it's closed
        if self.tx.is_closed() {
            std::task::Poll::Ready(Err(InProcessTransportError("Channel closed".to_string())))
        } else {
            std::task::Poll::Ready(Ok(()))
        }
    }

    fn start_send(
        self: std::pin::Pin<&mut Self>,
        item: TxJsonRpcMessage<R>,
    ) -> Result<(), Self::Error> {
        self.tx
            .try_send(item)
            .map_err(|e| InProcessTransportError(format!("Failed to send message: {}", e)))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // MPSC channels don't need explicit flushing
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // Let the sender be dropped naturally
        std::task::Poll::Ready(Ok(()))
    }
}

impl<R: ServiceRole> Stream for InProcessStream<R> {
    type Item = RxJsonRpcMessage<R>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.project().rx.poll_recv(cx)
    }
}

// Implement the IntoTransport trait for the unified InProcessTransport
impl<R: ServiceRole + 'static> IntoTransport<R, InProcessTransportError, ()>
    for InProcessTransport<R>
{
    fn into_transport(
        self,
    ) -> (
        impl Sink<TxJsonRpcMessage<R>, Error = InProcessTransportError> + Send + 'static,
        impl Stream<Item = RxJsonRpcMessage<R>> + Send + 'static,
    ) {
        // Simply split into sink and stream
        self.split()
    }
}

/// A convenience struct that manages an in-process service and provides a client transport
/// to communicate with it, similar to TokioChildProcess but for in-process communication
pub struct TokioInProcess<S>
where
    S: Service<RoleServer> + Send + 'static,
{
    server_service: Option<S>,
    client_transport: InProcessTransport<RoleClient>,
    buffer_size: usize,
    _server_task: Option<tokio::task::JoinHandle<()>>,
}

impl<S> TokioInProcess<S>
where
    S: Service<RoleServer> + Send + 'static,
{
    /// Create a new in-process service with the given service implementation
    pub fn new(service: S) -> Self {
        Self::with_buffer_size(service, 32)
    }

    /// Create a new in-process service with the given service implementation and buffer size
    pub fn with_buffer_size(service: S, buffer_size: usize) -> Self {
        Self {
            server_service: Some(service),
            client_transport: InProcessTransport {
                tx: tokio::sync::mpsc::channel(buffer_size).0,
                rx: tokio::sync::mpsc::channel(buffer_size).1,
                _phantom: PhantomData,
            },
            buffer_size,
            _server_task: None,
        }
    }

    /// Start the server and return a transport that can be used to communicate with it
    pub async fn serve(mut self) -> Result<Self, InProcessTransportError> {
        // Create the transport pair
        let (client_transport, server_transport) =
            create_in_process_transport_pair(self.buffer_size);

        // Start the server in a background task
        let server_service = self.server_service.take()
            .expect("Server service is missing - serve() was likely called twice");
        let server_handle = tokio::spawn(async move {
            if let Err(e) = server_transport.serve(server_service).await {
                tracing::error!("Server error: {:?}", e);
            }
        });

        // Replace the client transport with the newly created one
        self.client_transport = client_transport;
        self._server_task = Some(server_handle);

        Ok(self)
    }
}

impl<S> IntoTransport<RoleClient, InProcessTransportError, ()> for TokioInProcess<S>
where
    S: Service<RoleServer> + Send + 'static,
{
    fn into_transport(
        self,
    ) -> (
        impl Sink<TxJsonRpcMessage<RoleClient>, Error = InProcessTransportError> + Send + 'static,
        impl Stream<Item = RxJsonRpcMessage<RoleClient>> + Send + 'static,
    ) {
        self.client_transport.into_transport()
    }
}
