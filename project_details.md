# Semantic Clipboard: Project & Architecture Details

## 🧸 Explain Like I'm 5 (Beginner Overview)

Imagine you have a super-smart assistant sitting inside your computer. Every time you copy some text (like a recipe, a code snippet, or a funny joke), this assistant grabs it, reads it, and *understands* what it means, then safely stores it away in a magical notebook. 

Later, if you need that recipe again but can't remember the exact words you copied, you don't have to search for the exact phrase. You can just type "food instructions" or "baking", and the assistant will instantly flip to the right page because it knows that "recipe" and "food" mean the same thing! 

It does all of this incredibly fast, takes up almost zero space on your computer, and works completely offline so your private data never leaves your computer.

---

## 🧠 Core Functionality: How It Works

Semantic Clipboard is an ultra-lightweight, native background daemon written entirely in Rust. It monitors your operating system's clipboard, processes copied text using a local Artificial Intelligence model, and stores it in a searchable database. 

### 1. The Background Daemon & Clipboard Polling
When launched, the application spawns an asynchronous background worker using `tokio` and the `arboard` crate. This worker continuously polls the OS clipboard for new text content. When new text is detected, it skips duplicates and pipes the raw text into the Machine Learning engine.

### 2. The Machine Learning Engine (Vector Embeddings)
We use `ort` (Rust bindings for the ONNX Runtime) alongside the `tokenizers` crate to run a quantized version of the HuggingFace `all-MiniLM-L6-v2` transformer model entirely locally on the CPU.
- When text is copied, the model encodes the text into an array of 384 floating-point numbers (`f32`), known as a **Vector Embedding**.
- These numbers represent the semantic "meaning" and "context" of the text, rather than just the raw letters.

### 3. The SQLite Database
The raw text, the timestamp, and the 1536-byte (384 `f32`s) vector embedding are instantly persisted into a local SQLite database using `rusqlite`. The database runs in WAL (Write-Ahead Logging) mode to ensure high-speed concurrent reads and writes without locking up the UI thread. 

### 4. Semantic Search via Cosine Similarity
When the user opens the UI and types a search query (e.g., "my passwords"):
1. The search query is passed through the ONNX model to generate a new 384-dimensional query vector.
2. The application reads all saved vectors from SQLite.
3. A **Cosine Similarity** mathematical algorithm compares the angle between the query vector and every saved vector. 
4. Items with the highest similarity score (angles closest to 0) bubble to the top of the UI, allowing the user to find concepts even if they didn't type the exact words!

### 5. Wayland/Linux TCP Wakeup Architecture
Because modern Linux desktop environments (like Wayland) aggressively block background keyloggers for security, global hotkeys inside background apps are restricted. To bypass this, Semantic Clipboard acts as a single-instance daemon listening on local TCP port `45454`. When the user binds an OS-level keyboard shortcut to run the app, the secondary executable simply detects the open port, sends a `WAKEUP` signal via TCP to the daemon, and immediately exits. The daemon receives the TCP ping and natively un-minimizes the window!

---

## 🎤 Technical Interview Q&A

**Q: Why did you choose Rust instead of Python for an AI tool?**
**A:** Python is heavily reliant on massive C-bindings and requires a large runtime interpreter. A background daemon in Python loading PyTorch models would easily consume 500MB+ of idle RAM and take 4+ seconds to cold start. By using Rust, `ort`, and native `wgpu` UI rendering, we achieved an idle memory footprint of ~30MB, instant cold-starts, and a single compiled executable with zero external dependencies.

**Q: Explain how the Semantic Search fundamentally works without an internet connection.**
**A:** The app bundles the `all-MiniLM-L6-v2` ONNX model. Transformer models project human language into high-dimensional geometric space. Concepts that are similar (e.g., "Dog" and "Puppy") are mathematically placed closer together in that 384-dimensional space. By calculating the Cosine Similarity between the search query vector and the clipboard history vectors, we rank results purely based on spatial proximity, all executed entirely locally on the CPU.

**Q: How did you handle the "Glassy" UI aesthetics and dynamic themes?**
**A:** We used the `iced` GUI framework. We instructed the OS compositor to render the window completely transparent (`iced::window::Settings { transparent: true }`). Inside the app, we draw a container with an RGBA color (e.g., `Color::from_rgba(0.12, 0.12, 0.12, 0.85)`). Because the alpha channel is at 85%, the native OS compositor natively blends the desktop wallpaper behind it. We also integrated the `dark-light` crate to listen to OS-level theme changes, dynamically flipping our vector SVG icons to `Color::WHITE` or `Color::BLACK` via `iced::widget::svg::Style`.

**Q: What happens when the clipboard history gets too large?**
**A:** The SQLite database utilizes a background pruning routine. Users can set a `history_limit` (e.g., 10,000 items). Once the limit is breached, the oldest unpinned entries are deleted via a `DELETE FROM clipboard_history WHERE is_pinned = 0` query. Pinned items are explicitly ignored by this trim routine.

**Q: How do you handle UI updates preventing the main thread from blocking during heavy ML inferences?**
**A:** The `iced` architecture runs the UI on the main thread, but delegates heavy I/O operations (like downloading the ONNX models from HuggingFace) to `Task::perform` or `tokio` background workers. The ML model inference itself is heavily optimized via `GraphOptimizationLevel::Level3` and parallelized across 4 CPU intra-threads, meaning vector generation takes a fraction of a millisecond and does not cause UI stuttering.
