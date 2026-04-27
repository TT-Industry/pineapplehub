# Contributing to PineappleHub

## Prerequisites

- [Rust](https://rustup.rs/) (edition 2024)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev/): `cargo install trunk`

## Development

Start the dev server:

```bash
trunk serve -a 0.0.0.0
```

With release optimizations (recommended for testing performance-sensitive image processing):

```bash
trunk serve --release
```

> **Note:** The app requires `SharedArrayBuffer` for Rayon-based parallel processing. Trunk is configured to serve the necessary COOP/COEP headers via `Trunk.toml`.

## Mobile Development & Camera Testing

The camera capture feature (`Page::Camera`) uses `<input type="file" capture="environment">` and
requires the page to be served over **HTTPS** (or `localhost`) for `getUserMedia` and secure-context
APIs to be available.

### Option A — HTTPS with self-signed certificate (recommended for LAN testing)

1. Generate a certificate valid for your LAN IP (replace `192.168.x.x` with yours):

    ```bash
    openssl req -x509 -newkey rsa:2048 -keyout localhost.key \
      -out localhost.crt -days 365 -nodes -subj "/CN=192.168.x.x" \
      -addext "subjectAltName=IP:192.168.x.x,IP:127.0.0.1,DNS:localhost"
    ```

2. Start trunk with TLS:

    ```bash
    trunk serve -a 0.0.0.0 --release -p 8443 \
      --tls-key-path localhost.key \
      --tls-cert-path localhost.crt
    ```

3. On the phone, open `https://192.168.x.x:8443`. When warned about the untrusted
   certificate, tap **Advanced → Proceed**. The camera button will then work.

> **Note:** `localhost.key` and `localhost.crt` are excluded from version control
> via `.gitignore`. Regenerate them each time you need to test on a new device or
> after the cert expires.

### Option B — ngrok tunnel

```bash
ngrok http 8081
```

Use the `https://…ngrok.io` URL on the phone. No certificate setup needed.

## Chrome Remote Debugging on Android

Refer to [Remote debug Android devices](https://developer.chrome.com/docs/devtools/remote-debugging).

### Android Device

1. Enable **Developer options** and turn on **USB debugging**.
2. Connect the device to your development machine via USB.
3. Open Chrome on your Android device.

### Desktop OS

1. Open Chrome and navigate to `chrome://inspect/#devices`.
2. Ensure **Discover USB devices** is checked.
3. Accept the "Allow USB debugging" prompt on your Android screen.
4. _(Optional)_ To access your local `trunk` dev server from the phone, click **Port forwarding...** and map a mobile port to your local server (e.g., Port `8080` to IP address `localhost:8080`), then check **Enable port forwarding**.
5. Enter `localhost:8080` in the "Open tab with url" input and click **Open**.
6. Find your device and the opened tab in the list, then click **inspect** to open a DevTools window.

> **Note:** If your device doesn't show up in the list, you may need to download [platform-tools](https://developer.android.com/tools/releases/platform-tools) and run `adb devices` in your terminal to initialize the ADB daemon.
>
> **Wireless ADB Alternative (Android 11+)**: If USB debugging fails or is inconvenient:
>
> 1. Ensure your Android device and PC are on the same Wi-Fi network.
> 2. In Developer options, enable **Wireless debugging**.
> 3. Tap "Wireless debugging", then select **Pair device with pairing code**.
> 4. Run `adb pair <IP-address>:<port>` using the details and code shown on your screen.
> 5. Once paired, go back to the main "Wireless debugging" screen to find the _connection_ IP address and port (this port is usually different from the pairing port).
> 6. Run `adb connect <IP-address>:<port>` in your terminal.

## Project Structure

```
src/
├── main.rs              # Application entry, UI, and message loop
├── pipeline/
│   ├── mod.rs            # Pipeline types (FruitletMetrics, Step, etc.)
│   ├── fruitlet_counting.rs  # Interactive pipeline (with UI previews)
│   ├── fast.rs           # Headless pipeline (Web Worker parallel processing)
│   ├── scale_calibration.rs
│   └── roi_extraction.rs
├── export.rs             # CSV export
├── history/              # IndexedDB-backed analysis history
└── ui/                   # UI components
docs/
├── algorithms/           # Algorithm documentation (EN + ZH)
└── user_guide/           # Debug image interpretation guides
```

## Architecture Notes

- **Dual pipeline:** `fruitlet_counting.rs` is the interactive pipeline with iced UI types and `log` output; `fast.rs` is a pure-computation mirror (`Send + Sync`, no browser APIs) used by rayon Web Workers. **Changes to measurement logic must be synced between both files.**
- **WASM-only:** The crate targets `wasm32-unknown-unknown` exclusively. All dependencies must be WASM-compatible.
- **Lints:** `clippy::pedantic` and `clippy::all` are enabled as warnings. Run `cargo clippy --target wasm32-unknown-unknown` before submitting.

## Documentation

When modifying algorithms, update the corresponding documentation:

- [algorithm.md](docs/algorithms/algorithm.md) (English)
- [algorithm_zh.md](docs/algorithms/algorithm_zh.md) (Chinese)
- [debug_interpretation.md](docs/user_guide/debug_interpretation.md) (English)
- [debug_interpretation_zh.md](docs/user_guide/debug_interpretation_zh.md) (Chinese)
