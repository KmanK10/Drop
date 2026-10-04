// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "DropKit",
    platforms: [
        .iOS(.v17),
        .macOS(.v14),
    ],
    products: [
        .library(name: "DropKit", targets: ["DropKit"]),
    ],
    targets: [
        .target(
            name: "CArgon2",
            path: "Vendor/argon2",
            exclude: ["LICENSE"],
            sources: [
                "src/argon2.c",
                "src/core.c",
                "src/encoding.c",
                "src/ref.c",
                "src/thread.c",
                "src/blake2/blake2b.c",
            ],
            publicHeadersPath: "include",
            cSettings: [
                .headerSearchPath("include"),
                .headerSearchPath("src"),
                .headerSearchPath("src/blake2"),
            ]
        ),
        .target(
            name: "DropKit",
            dependencies: ["CArgon2"],
            path: "Sources/DropKit"
        ),
        .testTarget(
            name: "DropKitTests",
            dependencies: ["DropKit"],
            path: "Tests/DropKitTests"
        ),
    ]
)
