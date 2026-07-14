# Windows 11 Modern Context Menu — Experimental PoC

**Status: Experimental / Development-only. Not shipped in release builds.**

## Overview

Windows 11 replaced the classic context menu with a modern, streamlined menu.
Static registry verbs (like those registered by GeeZipX's Settings page and
NSIS installer) appear only in the "Show more options" sub-menu, requiring an
extra click.

This experimental PoC implements `IExplorerCommand` — the COM interface that
Windows 11 uses for **first-level** context menu entries — packaged as a
sparse MSIX package with a self-signed development certificate.

> **Important**: A self-signed certificate is NOT what makes items appear in
> the first-level menu.  The `IExplorerCommand` COM interface and the MSIX
> package identity are the mechanism.  The certificate only provides the
> package identity trust chain — in production, this must be an Azure
> Trusted Signing or CA-issued certificate.

## Architecture

```
Explorer right-clicks a file
        │
        ▼
Windows 11 Shell checks registered IExplorerCommand handlers
(discovered via MSIX desktop4:windows.fileExplorerContextMenus manifest)
        │
        ├── CLSID {A1...21} → ExtractCommand   → /extract
        ├── CLSID {9E...32} → ExtractHereCommand → /extract-here
        ├── CLSID {A0...43} → CompressZipCommand → /compress-zip
        └── CLSID {B1...54} → CompressCommand    → /compress
                │
                ▼
        geezipx_shell_extension.dll (COM surrogate in dllhost.exe)
                │
                │ GetState: check HKCU static verb keys
                │ Invoke: resolve paths, spawn geezipx-gui.exe <flag> <paths>
                ▼
        geezipx-gui.exe (same directory as DLL)
```

## Files

```
crates/shell-extension/
├── Cargo.toml                      # Windows-only cdylib, depends on windows 0.61
├── src/lib.rs                      # COM implementation (IExplorerCommand, IClassFactory)
└── package/
    └── AppxManifest.xml.in         # Sparse MSIX manifest template

scripts/windows-modern-menu/
├── build-dev-package.ps1           # Build DLL, create & sign MSIX package
├── register-dev-package.ps1        # Install cert + register sparse package
└── unregister-dev-package.ps1      # Remove package + optionally cert

.github/workflows/
└── windows-modern-menu-poc.yml     # CI: build, sign, register, verify, unregister
```

## Prerequisites

- Windows 11 (build 22000 or later)
- Windows 10/11 SDK (provides MakeAppx.exe, SignTool.exe)
- Rust with MSVC toolchain
- PowerShell 5.1+ (built-in on Windows)

## Development Installation

### Step 1: Build the package

```powershell
cd scripts/windows-modern-menu
.\build-dev-package.ps1 -Version "0.7.5"
```

This will:
1. Build `geezipx_shell_extension.dll` (release)
2. Generate a self-signed code-signing certificate in `Cert:\CurrentUser\My`
3. Create and sign a sparse MSIX package in `target/msix/`

### Step 2: Register the package

```powershell
.\register-dev-package.ps1
```

This will:
1. Ask for confirmation (self-signed certs have security implications — see below)
2. Import the certificate into `Cert:\CurrentUser\TrustedPeople` (NOT Root)
3. Register the sparse MSIX package with `Add-AppxPackage -ExternalLocation`
4. Print the registered package status

Use `-Force` for CI environments:
```powershell
.\register-dev-package.ps1 -Force
```

### Step 3: Restart Explorer

After registration, restart Explorer to see the changes:
```powershell
taskkill /f /im explorer.exe && start explorer.exe
```

Or log out and back in.

### Step 4: Verify

Right-click any archive file (`.zip`, `.7z`, `.rar` etc.) or any file/directory.
The modern context menu should now show GeeZipX verbs **at the first level**
(no "Show more options" required).

### Uninstall

```powershell
.\unregister-dev-package.ps1 [-RemoveCert]
```

## Security Considerations

### Self-Signed Certificate Risks

The PoC uses a self-signed code-signing certificate.  By importing it into
`TrustedPeople`, you are telling Windows to trust any package signed with this
certificate to run with the declared capabilities.

- The certificate is stored in `CurrentUser\TrustedPeople`, NOT
  `LocalMachine\Root` — it only affects your user account.
- The private key (PFX) is stored temporarily in `target/msix/` and is
  `.gitignore`d — it never leaves your machine.
- **Do NOT import self-signed certificates from untrusted sources into your
  TrustedPeople store.**

### Production Requirements

For an official release, this PoC must be adapted to use a trusted certificate:
- **Azure Trusted Signing** (recommended) — Microsoft's managed code-signing service
- **CA-issued code-signing certificate** — traditional EV/OV certificate from
  a public CA (DigiCert, Sectigo, etc.)
- **Microsoft Store publication** — packages submitted to the Store are
  signed by Microsoft

## Settings Synchronization

The `IExplorerCommand::GetState()` method checks whether the corresponding
static verb key exists under `HKCU\Software\Classes`.  This means the modern
menu items are **synchronized** with the GeeZipX Settings page:

- **Settings page enables a verb** → static HKCU key written → modern menu
  item visible (ECS_ENABLED)
- **Settings page disables a verb** → static HKCU key deleted → modern menu
  item hidden (ECS_HIDDEN)
- **Registry access error** → safe fallback: hidden

The NSIS installer's static verbs serve as a **fallback** — if the MSIX
package is not registered, the static verbs remain available in "Show more
options", and the GeeZipX Settings page continues to work.

## Fallback

| Scenario                          | Behavior                                               |
|-----------------------------------|--------------------------------------------------------|
| MSIX registered + verbs enabled   | Modern menu items visible at first level               |
| MSIX registered + verbs disabled  | No GeeZipX items in modern menu                        |
| MSIX not registered               | Static verbs in "Show more options" (Win10/Win11)     |
| Windows 10                        | Static verbs only (MSIX modern menu requires Win11)    |

## Verification Matrix

| Platform         | Modern menu | Static menu ("Show more") | Settings page toggle |
|------------------|-------------|---------------------------|---------------------|
| Windows 11 22000+| Yes (PoC)   | Yes (fallback)            | Controls both       |
| Windows 10       | No          | Yes                       | Controls static     |
| Linux            | N/A         | N/A                       | N/A                 |
| macOS            | N/A         | N/A                       | N/A                 |

## CI

The workflow `.github/workflows/windows-modern-menu-poc.yml` builds and
validates the PoC on every PR touching `crates/shell-extension/` or the
scripts, and on `workflow_dispatch`.  It does NOT modify the release pipeline.

The CI:
1. Runs `cargo test -p geezipx-shell-extension`
2. Builds `geezipx_shell_extension.dll`
3. Verifies DLL exports (`DllGetClassObject`, `DllCanUnloadNow`)
4. Generates an ephemeral self-signed certificate
5. Creates and signs a sparse MSIX package
6. Imports the cert into TrustedPeople, registers the package
7. Verifies all 4 CLSIDs via `Get-AppxPackageManifest`
8. Unregisters the package and removes the certificate
9. Uploads the DLL as a **DEVELOPMENT-ONLY** artifact (no MSIX/PFX/CER)

## Rollback

If the modern menu causes issues:
1. Run `.\unregister-dev-package.ps1 -RemoveCert`
2. Restart Explorer
3. The static verbs in "Show more options" remain fully functional

## Known Limitations

1. **Explorer restart required**: Menu changes require an Explorer restart (or
   logout/login) to take effect.  This differs from static verbs, which
   refresh immediately via `SHChangeNotify(SHCNE_ASSOCCHANGED)`.

2. **COM surrogate overhead**: Each right-click invocation starts a new
   `dllhost.exe` process for the COM surrogate.  This adds ~100-200ms of
   latency compared to in-proc COM or static verb execution.

3. **Sparse package cleanup**: `Remove-AppxPackage` removes the registration
   but does not delete the external files (DLL, GUI exe).  This is by design
   — the files belong to the main GeeZipX installation.

4. **Windows 10**: The `desktop4:windows.fileExplorerContextMenus` extension
   point requires Windows 11 (build 22000+).  The PoC does nothing on
   Windows 10 — users there continue to use static verbs via "Show more
   options".

5. **CLI flag compatibility**: The COM DLL passes the same CLI flags as the
   static verbs (`/extract`, `/extract-here`, `/compress-zip`, `/compress`).
   No changes to `geezipx-gui.exe` or the core engine are required.

## Future Work

- [ ] Migrate from `com:SurrogateServer` to in-proc COM if performance is
      acceptable and Explorer stability is verified
- [ ] Support dynamic submenus (`EnumSubCommands`) for advanced verb options
- [ ] Add MSIX to the NSIS installer (requires trusted certificate)
- [ ] Investigate `desktop4:FileExplorerContextMenus` with `DynamicVerbs` for
      format-aware menu filtering
- [ ] Add MSIX integration tests using the Windows App Certification Kit
- [ ] Evaluate sparse package identity impact on existing features
