// swift-tools-version:5.9
import PackageDescription

/// Where build.sh puts the network library, built from net/ for both architectures.
let network = Context.packageDirectory + "/build/net"

let package = Package(
    name: "Snowhush",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", exact: "2.10.0"),
    ],
    targets: [
        .systemLibrary(name: "CSnowhushNet", path: "Sources/CSnowhushNet"),
        .executableTarget(
            name: "Snowhush",
            dependencies: [.product(name: "Sparkle", package: "Sparkle"), "CSnowhushNet"],
            path: "Sources/Snowhush",
            // The per-frame loops reach class properties millions of times a second; Swift's runtime exclusivity
            // checks around each access cost as much as the physics itself.
            swiftSettings: [.unsafeFlags(["-enforce-exclusivity=unchecked"], .when(configuration: .release))],
            linkerSettings: [
                .unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks", "-L", network]),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("Security"),
            ]
        ),
    ]
)
