# Orbit for Windows

A native Windows window manager inspired by Loop's hold, point, and release interaction. This is an independent implementation; it does not incorporate Loop's Swift source or branding.

**Version 0.1.0 — development preview for Windows 11 x64.** The executable has been cross-built on macOS. It has not been launched or visually tested on Windows. Compilation and core tests do not establish Windows runtime acceptance. See [WINDOWS-QA.md](../WINDOWS-QA.md).

## Gift installer

Open **Orbit-Gift-0.1.0-x64.exe**, tear the cobalt wrapping with a right-to-left drag (or choose the keyboard-accessible button), select optional desktop/sign-in shortcuts, then install and launch. The native Rust bootstrap embeds the complete app and needs no downloads. It installs for the current user, registers Start and Installed apps entries, and includes Hebrew instructions, licenses and diagnostics. Existing preferences are preserved. `Orbit-Gift-Preview.html` is a standalone browser demonstration that performs no installation.

Build the gift with `powershell -NoProfile -File scripts/build-gift.ps1`. Both the bootstrap and embedded application have their own manifest and icon. Gift installation and launch still require interactive Windows verification.

## Try it

Extract the complete portable ZIP into a folder, then open **Orbit.exe**. Settings appears on first launch. Save your preferences, close Settings, and select an application window.

1. Hold **Right Ctrl** for a moment.
2. Move the pointer toward a layout in the ring.
3. Release Right Ctrl to arrange the selected window.

Moving back to the center before release cancels. Escape or a mouse click also cancels; the cancelling click is consumed so it does not act on the application underneath. A quick Ctrl tap or a normal Ctrl shortcut before the hold delay passes does not move a window.

The preview is unsigned. Windows may show an unknown-publisher/SmartScreen prompt. It requires no browser, .NET installation, account, or internet connection. Only run this development build if you trust its source.

## Controls

| Input | Action |
| --- | --- |
| Hold Right Ctrl | Open the radial menu after the configured delay |
| Win + Alt + Space | Open a menu without holding a trigger; Enter applies |
| Escape | Cancel |
| Move toward the ring's top | Maximize with Windows' native maximize command |
| Move left / right | Left / right half |
| Move toward a corner | Corresponding quarter |
| Move downward | Center using the window's restored size |
| Mouse wheel, with left/right selected | Half → two thirds → one third |
| Mouse wheel, with top selected | Maximize → top half → bottom half |
| Tab / Shift + Tab | Preview the layout on the next / previous display |
| Arrow keys | Select left half, right half, maximize, or center |
| Number keys 7 / 9 / 1 / 3 | Select the four corners |
| Number keys 8 / 4 / 6 / 2 / 5 | Maximize / left / right / center / center |
| Enter while menu is open | Apply selected layout |
| Z while menu is open, or Win + Alt + Z | Undo the last Orbit arrangement |

Orbit keeps up to 32 undo records for the current run. Closing and reopening Orbit clears this history. Windows' built-in Win + arrow snapping and Win + Shift + arrow monitor movement are left available outside Orbit's interaction.

**Tray:** left-click Orbit's icon to open Settings. Right-click for pause/resume, undo, administrator restart, and quit. The icon may be in the notification area's overflow menu.

**Settings:** choose Right Ctrl, F8, or F9; set hold delay, window gap, preview, and sign-in startup. F8/F9 become dedicated trigger keys while Orbit is active. Save applies the settings. Right Ctrl remains an ordinary modifier for other shortcuts; combinations pressed after the menu is already open operate the menu.

## Install and uninstall

**Install.cmd**, the **Install** button, or `Orbit.exe --install` installs for the current Windows user in `%LOCALAPPDATA%\Programs\Orbit`, adds Start menu shortcuts, and registers Orbit in Installed apps. No administrator access is requested. Open Orbit from Start afterward. Enable automatic sign-in startup explicitly in Settings; installation alone does not enable it.

To remove an installed copy, quit Orbit from its tray menu and choose **Uninstall Orbit** in Start or uninstall it from Windows Settings. Portable copies can be removed after quitting. Settings, the local log, and diagnostic files remain in `%LOCALAPPDATA%\Orbit`; remove that folder manually to erase them.

Administrator applications need the same permission level. Orbit reports the mismatch and offers **Restart as administrator** in its tray menu; Windows shows the normal UAC prompt. Ordinary use stays unelevated. Elevated applications, UAC, fullscreen games, protected applications and custom window styles require interactive testing; they are not claimed as universally supported.

## Diagnostics and privacy

Run **Check-Windows-Startup.cmd**, `Orbit.exe --diagnose`, or Settings → Diagnostics. The report opens at `%LOCALAPPDATA%\Orbit\diagnostics.json`. It checks initialization of the renderer, overlays, hooks, hotkeys, DPI awareness and monitor geometry. Startup failures are written there as errors. If Orbit is already running, its instance generates the report.

The report does not test actual rendering or prove that hooks are still receiving events. No keys, typed content, clipboard, application names or window titles are recorded. The bounded local log contains startup/error messages. Orbit has no telemetry or network code.

## Implementation

- Windows-only Rust with Microsoft Windows API bindings.
- Win32 windows, common controls, notification icon, registry startup and Shell shortcuts.
- Direct2D + DirectWrite draw into premultiplied-alpha layered windows.
- Non-activating, click-through overlays; focus and target process are checked before applying.
- A dedicated input thread queues events; pointer messages are coalesced. No rendering or input polling occurs while idle. A short watchdog timer runs only during a gesture, and a one-shot timer checks placement after applying.
- Per-monitor-v2 manifest, monitor work areas, DPI-scaled gaps, DWM resize-border compensation, native maximize and window placement for undo.

Snapping here means requesting window positions through Win32. Registration with Windows Snap Layouts, Snap Assist groups, full Loop feature parity, automatic updates and measured performance are not part of this preview.

## Build on Windows

Install Rust 1.93 or later with the MSVC toolchain, plus Visual Studio Build Tools' **Desktop development with C++** workload and a Windows SDK.

```powershell
cargo build --locked --release
# target\release\orbit.exe
powershell -NoProfile -File scripts\check.ps1
```

The build script embeds the icon, version metadata, Common Controls v6, an as-invoker execution level and per-monitor-v2 DPI awareness. The original icon can be regenerated with `python scripts/create_icon.py`. The optional `windows-linker.py`/`windows-dlltool.py` wrappers permit cross-building with Zig 0.13 and Rust's `x86_64-pc-windows-gnullvm` target; installed users need neither tool.

## License

Orbit's source is MIT licensed. Dependency notices are in THIRD-PARTY-NOTICES.txt. Loop is the interaction inspiration and remains a separate project: [mrkai77/Loop](https://github.com/mrkai77/Loop).
