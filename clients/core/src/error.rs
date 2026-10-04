use thiserror::Error;

#[derive(Debug, Error)]
pub enum DropError {
    #[error("Enter a server address starting with https://")]
    BadServer,
    #[error("Enter the username you were given.")]
    BadUsername,
    #[error("Enter your password.")]
    EmptyPassword,
    #[error("That password is too long.")]
    LongPassword,
    #[error("Key derivation parameters are too weak.")]
    WeakKdf,
    #[error("Unsupported key derivation.")]
    BadKdf,
    #[error("The password didn't unlock this account.")]
    KeyCheck,
    #[error("Sign-in didn't stick. Try again.")]
    SessionLost,
    #[error("Sign in again.")]
    SignedOut,
    #[error("Sign in before adding items.")]
    Locked,
    #[error("Write something first.")]
    EmptyText,
    #[error("That name is too long.")]
    NameTooLong,
    #[error("That file type is too long.")]
    MimeTooLong,
    #[error("Item format is not recognized.")]
    BadItem,
    #[error("Item is truncated.")]
    TruncatedItem,
    #[error("That item is not here.")]
    Missing,
    #[error("This file can't be copied.")]
    CantCopy,
    #[error("Couldn't copy that.")]
    Clipboard,
    #[error("Folders aren't uploaded. Drop the files inside.")]
    Directory,
    #[error("Can't reach the server.")]
    Network,
    #[error("{0}")]
    Server(String),
    #[error("{0}")]
    Message(String),
}
