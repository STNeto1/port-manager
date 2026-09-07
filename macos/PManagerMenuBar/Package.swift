// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "PManagerMenuBar",
    platforms: [.macOS(.v13)],
    targets: [
        .target(
            name: "PManagerCore",
            path: "Sources/PManagerCore"
        ),
        .executableTarget(
            name: "PManagerMenuBar",
            dependencies: ["PManagerCore"],
            path: "Sources/PManagerMenuBar"
        ),
    ]
)
