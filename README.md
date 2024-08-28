# radarsync: CLI utility for transferring music to Doppler

⚠️ NOTE: This is **NOT** an official tool for syncing music with Doppler. It uses undocumented server APIs which are subject to change at any time.

---

radarsync is intended to transfer files to [Doppler for iOS] where the [macOS app] and [Wi-Fi Transfer website] aren't feasible to run, such as remote servers. The library component, `doppler-ws`, uses the websocket API created for the Wi-Fi Transfer website to pair with the device and upload music files.

## Installation

Assuming you have cargo installed, the package can be built from source:

```
git clone https://github.com/flurrikat/radarsync
cd radarsync
cargo install --path radarsync --locked
```

## Usage

In the Doppler app, switch to the Import tab and choose "Import from Wi-Fi". You'll be prompted to scan a QR code. Run radarsync without choosing a device to get a pairing QR code. For example, to send one file:

```
radarsync "My Song.m4a"
```

The six digit code can also be used from the same page. You can hide the QR code with the `--no-qr` argument.

## Known issues and caveats

- Neither radarsync nor the app check what files have already been transferred, so sending a music file multiple times will result in duplicate entries.
- There is no feedback from our end when a file is transferred as to whether it failed to import in the app. Make sure to keep an eye on the app to see if files fail (show a red X).
- Using "Import from Wi-Fi" on first run causes every uploaded file to fail. This can be worked around by continuing without importing, then going to the Import tab and uploading. I haven't yet figured out if this is a Doppler issue or our use of the API.

[Doppler for iOS]: https://brushedtype.co/doppler/
[macOS app]: https://brushedtype.co/doppler-transfer/
[Wi-Fi Transfer website]: https://doppler-transfer.com/
