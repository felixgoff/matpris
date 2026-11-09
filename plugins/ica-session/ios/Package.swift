// swift-tools-version:5.3

import PackageDescription

let package = Package(
  name: "tauri-plugin-ica-session",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v16),
  ],
  products: [
    .library(
      name: "tauri-plugin-ica-session",
      type: .static,
      targets: ["tauri-plugin-ica-session"])
  ],
  dependencies: [
    // Copied here by the plugin's build script (tauri-plugin) when building for iOS.
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-ica-session",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
