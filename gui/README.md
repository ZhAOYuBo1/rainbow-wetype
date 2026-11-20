# rainbow-wetype GUI

Native Windows GUI for the Rust `rainbow-wetype` CLI.

## Build

```powershell
cargo build --release
```

The build needs the Windows SDK `rc.exe` (for the embedded manifest and icon).
`winres` normally finds it automatically; if not, add the SDK `bin\x64` directory
to `PATH`.

## Release layout

Ship these together:

```text
rainbow-wetype-gui.exe
wetype-color.exe
signatures.json
```

The GUI has a `requireAdministrator` manifest, so Windows shows UAC at launch.
The child CLI inherits that elevated context.

## Notes

- GUI calls the Rust CLI, which remains the single implementation of scanning,
  backup, apply, restore and verification.
- The CLI output is JSON; the GUI only renders/controls it.
- The bundled icon is a local design based on the WeType logo plus a band-aid;
  keep the non-official disclaimer if distributing.
