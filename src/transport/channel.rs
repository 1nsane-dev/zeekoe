use {
    super::{Handshake, SessionKey},
    dialectic::Chan,
    dialectic_reconnect::{resume, retry},
    dialectic_tokio_serde::{codec::LengthDelimitedCodec, Receiver, Sender, SymmetricalError},
    dialectic_tokio_serde_bincode::Bincode,
    std::io,
    tokio::{
        io::{ReadHalf, WriteHalf},
        net::TcpStream,
    },
    tokio_rustls::webpki::DNSName,
};

/// A *server-side* session-typed channel over TCP using length-delimited bincode encoding for
/// serialization.
pub type ServerChan<S> = ResumeSplitChan<
    S,
    SessionKey,
    Bincode,
    LengthDelimitedCodec,
    tokio_rustls::server::TlsStream<TcpStream>,
>;

/// A *client-side* session-typed channel over TCP using length-delimited bincode encoding for
/// serialization.
pub type ClientChan<S> = RetrySplitChan<
    S,
    SessionKey,
    Handshake,
    (DNSName, u16),
    io::Error,
    dialectic_tokio_serde::SymmetricalError<Bincode, LengthDelimitedCodec>,
    Bincode,
    LengthDelimitedCodec,
    tokio_rustls::client::TlsStream<TcpStream>,
>;

/// An error in the underlying non-resuming transport.
pub type TransportError = SymmetricalError<Bincode, LengthDelimitedCodec>;

type ResumeSplitChan<S, K, F, E, T> =
    Chan<S, ResumeSplitSender<K, F, E, T>, ResumeSplitReceiver<K, F, E, T>>;

type RetrySplitChan<S, K, H, A, CErr, HErr, F, E, T> = Chan<
    S,
    RetrySplitSender<K, H, A, CErr, HErr, F, E, T>,
    RetrySplitReceiver<K, H, A, CErr, HErr, F, E, T>,
>;

type ResumeSplitSender<K, F, E, T> =
    resume::Sender<K, SplitSender<F, E, T>, SplitReceiver<F, E, T>>;
type ResumeSplitReceiver<K, F, E, T> =
    resume::Receiver<K, SplitSender<F, E, T>, SplitReceiver<F, E, T>>;

type RetrySplitSender<K, H, A, CErr, HErr, F, E, T> =
    retry::Sender<H, A, K, CErr, HErr, SplitSender<F, E, T>, SplitReceiver<F, E, T>>;
type RetrySplitReceiver<K, H, A, CErr, HErr, F, E, T> =
    retry::Receiver<H, A, K, CErr, HErr, SplitSender<F, E, T>, SplitReceiver<F, E, T>>;

type SplitSender<F, E, T> = Sender<F, E, WriteHalf<T>>;
type SplitReceiver<F, E, T> = Receiver<F, E, ReadHalf<T>>;
