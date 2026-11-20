# Installing Tear on Windows

Tear is currently a **preview for Windows 10 and 11 (64-bit)**. macOS and Linux versions are planned.

## 1. Download

Download **`tear-windows-x64.zip`** from the [latest release](https://github.com/Xyst69/tear/releases) or from the [website](https://xyst69.github.io/tear/#download).

## 2. Unblock the download

Tear isn't code-signed yet, so Windows marks the download as untrusted.

1. Right-click `tear-windows-x64.zip` and choose **Properties**.
2. At the bottom of the **General** tab, tick **Unblock** and click **OK**.

## 3. Extract the zip

Right-click `tear-windows-x64.zip`, choose **Extract All…**, then click **Extract**.

## 4. Run the installer

Open the extracted folder and double-click **`Install.cmd`**.

- If **Windows protected your PC** appears, click **More info**, then **Run anyway**.
- A window confirms the installation. Press any key to close it.

The installer:

- copies Tear to `%LOCALAPPDATA%\Programs\tear`
- adds **Tear** to the right-click menu of archive files, for your user account only
- adds `tear` to your user `PATH` for use in a terminal

No administrator rights are needed. Nothing runs in the background.

## 5. Use it

Right-click any `.zip`, `.7z`, `.rar`, `.tar`, `.tar.gz`, `.tar.xz`, `.tar.bz2`, `.tar.zst`, `.gz`, `.xz`, `.bz2` or `.zst` file:

```
Tear  ›  Extract here
         Extract to new folder
         Test archive
```

On **Windows 11**, choose **Show more options** first, or select the file and press **Shift+F10**.

| Choice | What happens |
|---|---|
| Extract here | If the archive holds one folder, you get that folder. Loose files go into a new folder named after the archive. |
| Extract to new folder | Always creates a folder named after the archive. |
| Test archive | Checks every file without extracting and tells you the result. |

Tear never overwrites existing files: extracting twice gives `name (2)`. Password-protected archives ask for the password, and a wrong password lets you try again. A successful extraction shows no window at all.

### From a terminal

Open a new terminal after installing:

```powershell
tear x archive.zip                  # smart extract next to the archive
tear x archive.7z --folder          # always into a new folder
tear x archive.rar -o D:\Games      # choose the destination
tear x secret.zip -p PASSWORD       # supply a password
tear l archive.zip                  # list the contents
tear t archive.zip                  # verify without extracting
tear --help
```

Exit codes: `0` success, `1` some files were damaged (the rest were extracted), `2` invalid input, `3` missing or wrong password.

## Updating

Download the new release and run its `Install.cmd`. It replaces the previous version.

## Uninstalling

Double-click **`Uninstall.cmd`** in the extracted folder or in `%LOCALAPPDATA%\Programs\tear`. It removes the menu entries, the `PATH` entry and the installed files.

## Verifying the download (optional)

Each release includes `SHA256SUMS`. In PowerShell:

```powershell
(Get-FileHash .\tear-windows-x64.zip -Algorithm SHA256).Hash
```

The result should match the value in `SHA256SUMS` (letter case doesn't matter).

## Problems

[Open an issue](https://github.com/Xyst69/tear/issues) with the archive type, what you clicked, and the exact message you saw.
