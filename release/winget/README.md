# Winget Package Submission Guide

## Prerequisites

1. Fork https://github.com/microsoft/winget-pkgs
2. Install Windows Package Manager Manifest Creator:
   ```
   winget install Microsoft.WindowsPackageManagerManifestCreator
   ```

## Submission Steps

1. **Build the release zip**:
   ```powershell
   # On a Windows machine:
   cd agent
   cargo build --release -p xai-grok-pager-bin
   # Package everything:
   .\scripts\build-windows-package.sh  # or run manually
   ```

2. **Compute SHA-256**:
   ```powershell
   Get-FileHash lumen-windows-x86_64.zip -Algorithm SHA256
   ```

3. **Update the manifest**:
   Replace `<INSERT_SHA256_HERE>` in `Lumen.Lumen.yaml` with the actual hash.

4. **Submit to winget-pkgs**:
   ```powershell
   git clone https://github.com/microsoft/winget-pkgs
   cd winget-pkgs
   # Copy manifest to correct location:
   # manifests/l/Lumen/Lumen/0.1.250/
   git add .
   git commit -m "Add Lumen.Lumen version 0.1.250"
   git push
   # Create PR on GitHub
   ```

5. **Once merged**, users can install with:
   ```
   winget install Lumen.Lumen
   ```

## Automated Submission (Future)

Once GitHub Actions CI is configured, the release workflow will:
1. Build Windows binary
2. Compute SHA-256
3. Update the manifest
4. Open a PR to winget-pkgs automatically
