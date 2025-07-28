use std::{error::Error, fmt, marker::PhantomData};

use futures::{Sink, Stream};
use tokio::sync::mpsc::{self, Receiver, Sender};

use rmcp::{
    Service, serve_client, serve_server,
    service::{
        ClientInitializeError, RoleClient, RoleServer, RunningService, RxJsonRpcMessage,
        ServerInitializeError, ServiceRole, TxJsonRpcMessage,
    },
    transport::{IntoTransport, sink_stream::SinkStreamTransport},
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

impl From<ClientInitializeError> for InProcessTransportError {
    fn from(err: ClientInitializeError) -> Self {
        InProcessTransportError(format!("Client initialization error: {}", err))
    }
}

impl From<ServerInitializeError> for InProcessTransportError {
    fn from(err: ServerInitializeError) -> Self {
        InProcessTransportError(format!("Server initialization error: {}", err))
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
            pending: None,
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
    /// The error type returned by serve operations
    type Error;

    /// Create a new service using this transport
    fn serve<S>(
        self,
        service: S,
    ) -> impl std::future::Future<Output = Result<RunningService<R, S>, Self::Error>> + Send
    where
        S: Service<R> + Send + 'static;
}

impl InProcessTransportExt<RoleClient> for InProcessTransport<RoleClient> {
    type Error = ClientInitializeError;

    async fn serve<S>(
        self,
        service: S,
    ) -> Result<RunningService<RoleClient, S>, ClientInitializeError>
    where
        S: Service<RoleClient> + Send + 'static,
    {
        serve_client(service, self).await
    }
}

impl InProcessTransportExt<RoleServer> for InProcessTransport<RoleServer> {
    type Error = ServerInitializeError;

    async fn serve<S>(
        self,
        service: S,
    ) -> Result<RunningService<RoleServer, S>, ServerInitializeError>
    where
        S: Service<RoleServer> + Send + 'static,
    {
        serve_server(service, self).await
    }
}

pin_project_lite::pin_project! {
    /// The sink component of the in-process transport
    pub struct InProcessSink<R: ServiceRole> {
        tx: Sender<TxJsonRpcMessage<R>>,
        _phantom: PhantomData<R>,
        // Store any pending message that couldn't be sent
        pending: Option<TxJsonRpcMessage<R>>,
    }
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
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        let this = self.project();

        // First, try to flush any pending message
        if let Some(pending) = this.pending.take() {
            match this.tx.try_send(pending) {
                Ok(()) => {
                    // Successfully sent pending message, now we're ready
                    std::task::Poll::Ready(Ok(()))
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(msg)) => {
                    // Channel still full, store the message back and return Pending
                    *this.pending = Some(msg);
                    // Register waker for potential future readiness
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => std::task::Poll::Ready(
                    Err(InProcessTransportError("Channel closed".to_string())),
                ),
            }
        } else {
            // No pending message, check if channel is closed
            if this.tx.is_closed() {
                std::task::Poll::Ready(Err(InProcessTransportError("Channel closed".to_string())))
            } else {
                // Channel is open and no pending messages, we're ready
                std::task::Poll::Ready(Ok(()))
            }
        }
    }

    fn start_send(
        self: std::pin::Pin<&mut Self>,
        item: TxJsonRpcMessage<R>,
    ) -> Result<(), Self::Error> {
        let this = self.project();

        // Try to send immediately
        match this.tx.try_send(item) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(msg)) => {
                // Channel full, store message as pending for next poll_ready
                *this.pending = Some(msg);
                Ok(())
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Err(InProcessTransportError("Channel closed".to_string()))
            }
        }
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

// Use the TransportAdapterSinkStream to create a proper Transport
pub enum TransportAdapterInProcess {}

// Implement the IntoTransport trait for the unified InProcessTransport
impl<R> IntoTransport<R, InProcessTransportError, TransportAdapterInProcess>
    for InProcessTransport<R>
where
    R: ServiceRole + 'static + Unpin,
    R::Req: Unpin,
    R::Resp: Unpin,
    R::Not: Unpin,
{
    fn into_transport(
        self,
    ) -> impl rmcp::transport::Transport<R, Error = InProcessTransportError> + 'static {
        let (sink, stream) = self.split();
        SinkStreamTransport::new(sink, stream)
    }
}

/// A convenience struct that manages an in-process service and provides a client transport
/// to communicate with it, similar to TokioChildProcess but for in-process communication
pub struct TokioInProcess {
    client_transport: InProcessTransport<RoleClient>,
    _server_task: tokio::task::JoinHandle<()>,
}

impl TokioInProcess {
    /// Create a new in-process service with the given service implementation
    pub async fn new<S>(service: S) -> Result<Self, InProcessTransportError>
    where
        S: Service<RoleServer> + Send + 'static,
    {
        Self::with_buffer_size(service, 32).await
    }

    /// Create a new in-process service with the given service implementation and buffer size
    pub async fn with_buffer_size<S>(
        service: S,
        buffer_size: usize,
    ) -> Result<Self, InProcessTransportError>
    where
        S: Service<RoleServer> + Send + 'static,
    {
        // Create the transport pair
        let (client_transport, server_transport) = create_in_process_transport_pair(buffer_size);

        // Start the server in a background task
        let server_handle = tokio::spawn(async move {
            tracing::debug!("Server task starting");
            match server_transport.serve(service).await {
                Ok(running_service) => {
                    tracing::debug!("Server initialized successfully, keeping it alive");
                    // Keep the running service alive by waiting for it to finish
                    if let Err(e) = running_service.waiting().await {
                        tracing::error!("Server service error: {:?}", e);
                    } else {
                        tracing::debug!("Server service completed normally");
                    }
                }
                Err(e) => {
                    tracing::error!("Server initialization error: {:?}", e);
                }
            }
        });

        Ok(Self {
            client_transport,
            _server_task: server_handle,
        })
    }
}

impl IntoTransport<RoleClient, InProcessTransportError, TransportAdapterInProcess>
    for TokioInProcess
{
    fn into_transport(
        self,
    ) -> impl rmcp::transport::Transport<RoleClient, Error = InProcessTransportError> + 'static
    {
        // Spawn a task to keep the server task alive
        tracing::debug!("Moving server task to background keeper");
        tokio::spawn(async move {
            tracing::debug!("Background keeper task started");
            if let Err(e) = self._server_task.await {
                tracing::error!("Server task failed: {:?}", e);
            } else {
                tracing::debug!("Background server task completed normally");
            }
        });

        self.client_transport.into_transport()
    }
}
