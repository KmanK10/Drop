import Foundation

public enum DropError: Error, Equatable {
    case badServer
    case badUsername
    case emptyPassword
    case longPassword
    case weakKdf
    case badKdf
    case keyCheck
    case sessionLost
    case signedOut
    case locked
    case emptyText
    case nameTooLong
    case mimeTooLong
    case badItem
    case truncatedItem
    case missing
    case cantCopy
    case directory
    case network
    case server(String)
    case message(String)

    public var text: String {
        switch self {
        case .badServer: return "Enter a server address starting with https://"
        case .badUsername: return "Enter the username you were given."
        case .emptyPassword: return "Enter your password."
        case .longPassword: return "That password is too long."
        case .weakKdf: return "Key derivation parameters are too weak."
        case .badKdf: return "Unsupported key derivation."
        case .keyCheck: return "The password didn't unlock this account."
        case .sessionLost: return "Sign-in didn't stick. Try again."
        case .signedOut: return "Sign in again."
        case .locked: return "Sign in before adding items."
        case .emptyText: return "Write something first."
        case .nameTooLong: return "That name is too long."
        case .mimeTooLong: return "That file type is too long."
        case .badItem: return "Item format is not recognized."
        case .truncatedItem: return "Item is truncated."
        case .missing: return "That item is not here."
        case .cantCopy: return "This file can't be copied."
        case .directory: return "Folders aren't uploaded. Share the files inside."
        case .network: return "Can't reach the server."
        case .server(let message): return message
        case .message(let message): return message
        }
    }
}
