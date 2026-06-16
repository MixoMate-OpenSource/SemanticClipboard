# 🦀 SemanticClipboard

> **Ultra-Lightweight, 100% Rust-Native Semantic Clipboard Manager with Hardened Reboot Survival.**  
> A zero-dependency, cross-platform utility engineered to run in the background with a minimal footprint (~30MB RAM). Replaces Windows `Win + V` with concept-driven semantic search that persists automatically across system reboots.

---

## 🌟 Performance Metrics

| Feature | Python Implementation | Rust Native Implementation |
| :--- | :--- | :--- |
| **Idle Memory Consumption**| ~450 MB - 600 MB RAM | **~25 MB - 40 MB RAM** 🚀 |
| **Cold Startup Overhead**  | 4.2 Seconds (PyTorch init) | **0.15 Seconds (Instant)** ⚡ |
| **Binary Packaging**       | Fragmented interpreter files | **Single native executable** 📦 |

---

## 🎨 Modern UI Features
- **Glassy Aesthetics & Invisible Scrollbars:** Beautiful layered RGBA transparency that natively blurs your desktop wallpaper under the UI container. Desktop scrollbars are entirely hidden for a premium, borderless feel while preserving fluid trackpad/wheel navigation.
- **Dynamic System Themes:** Automatically detects and conforms to your operating system's native Light or Dark mode preferences in real-time.
- **Sleek Vector Icons:** Fully bundled, modern SVG iconography natively rendered directly within the binary for pixel-perfect clarity without any external asset dependencies.

---

## 🚀 Quick Setup & Compilation

### Prerequisites
*   [Rust Compiler and Toolchain (MSRV 1.75+)](https://rust-lang.org)
*   *Linux Users Only*: Ensure native development packages are installed for X11/Wayland clipboard support:
    ```bash
    sudo apt-get install libx11-dev libxtst-dev libxmu-dev xclip xsel
    ```

### Compilation Pipeline
1. Clone the repository and navigate into the project workspace:
   ```bash
   git clone https://github.com/Mixomate/SemanticClipboard.git
   cd SemanticClipboard
   ```
2. Compile and link the native high-speed release binary profile:
   ```bash
   cargo build --release
   ```
3. Run your optimized standalone system service:
   ```bash
   ./target/release/SemanticClipboard
   ```

---

## ⌨️ Controls & Platform Guidelines

Semantic Clipboard operates as a **single-instance background daemon**. Attempting to run the executable a second time will simply securely "wake up" the hidden primary instance instantly.

*   **Windows / macOS**: Double-tap `Ctrl + Alt` to toggle the floating search canvas.
*   **Linux / Wayland**: Due to modern Wayland security protocols blocking global keyloggers, background hotkey listeners are restricted. **Solution:** Use your Desktop Environment's custom keyboard settings (e.g., GNOME Keyboard Shortcuts) and bind a hotkey (like `Ctrl+Alt+V`) to directly execute the `SemanticClipboard` command.
*   **System Tray (Optional):** The app runs natively as a background process and can optionally place an icon in your OS System Tray. Clicking the tray icon instantly brings up the clipboard interface.
*   `Click Outside to Hide`: The UI automatically dismisses itself completely out of your way the moment you click away, meaning it never clutters your taskbar.
*   `Double-Click List Item` or `Copy Icon`: Instantly re-copies the historical record and minimizes the application window.
*   `Pin Icon`: Toggles reboot protection on the item, locking it against background database auto-trim routines. **Pinned items are always grouped at the top of the UI.**
*   `Lock Icon`: Obscure the entry for privacy. The UI will securely mask the content using asterisks (`My***rd`) and place your custom label underneath it in smaller, faded text for easy identification. **Note:** Once an item is obscured, it stays obscured for maximum privacy (the lock icon becomes grayed out). To remove it, delete the item entirely.

---

## 📦 Multi-Platform Packaging

This project supports native installers via [`cargo-packager`](https://github.com/tauri-apps/cargo-packager), integrated natively into `Cargo.toml`. 

To generate a native installer for your *current* operating system:
```bash
cargo install cargo-packager
cargo packager --release
```

### Linux Targets (Fedora, Ubuntu, Arch)
For Linux environments, you can automatically build `.deb`, `.AppImage`, and `.tar.gz` files using the `pacman` format.

If you are generating an `.AppImage` on a modern Linux distribution (like Fedora 40 or Ubuntu 24.04), `linuxdeploy`'s internal binary stripper may fail on modern `.relr.dyn` architectures. Bypass this by setting the `NO_STRIP` environment variable:
```bash
NO_STRIP=true cargo packager --release -f deb,appimage,pacman
```

**Fedora / RedHat (.rpm):**
`cargo-packager` does not natively generate `.rpm` files. To generate a native RedHat `.rpm` package natively in Rust, use the bundled `build_rpm.sh` script, which automates the use of `cargo-generate-rpm`:
```bash
./build_rpm.sh
```
This script will automatically install dependencies, compile, strip the binary, generate the `.rpm`, and tell you where to find the installer!

### CI/CD (GitHub Actions)
Since you cannot natively build `.dmg` (macOS) or `.msi` (Windows) installers from a Linux machine, the recommended approach for distributing this app is through **GitHub Actions**. By setting up a matrix workflow, GitHub's cloud runners will automatically compile the release targets for Windows, macOS, and Linux simultaneously whenever you push code.

---

## 📄 License
Distributed under the MIT License. See `LICENSE` for details.
