# Windows acceptance checks

Status: **pending real Windows execution**. No Windows screenshots, runtime timings, focus acceptance or installation acceptance were obtained on the macOS build host.

Run this on Windows 11 x64. Keep the resulting diagnostics JSON with any observations. Record the application version, Windows build, display scaling, and affected application when reporting a failure. Do not include private typed content or window titles.

1. **Start:** open the portable executable, verify Settings and the tray icon appear, and run diagnostics. Launch a second copy; it should open the existing settings window. Quit should remove the icon and stop all hooks.
2. **Normal typing:** in Notepad and a browser, tap Right Ctrl, use Right Ctrl+C/V quickly, hold Ctrl while typing a shortcut, and release it. No windows should move and no keys should remain stuck. Repeat after cancelling the menu and changing the trigger to F8/F9.
3. **Gesture:** select another app; hold the trigger, move in each of eight directions, release, and compare the window's visible frame with the preview. Returning to the center, Escape and a click should cancel. The cancelling click must not reach the app underneath. Verify arrows, number keys and Enter, including key repeat and key release after cancellation.
4. **Fast release:** move during the opening delay, stop, then release. Check that opening evaluates the latest position and that a quick tap never moves a window. Check fast pointer movement followed immediately by key release.
5. **Focus:** the ring and preview must never take focus. Alt+Tab, opening another application, locking the PC, sleep/wake, or disconnecting a display during a gesture must cancel safely. Start a new gesture afterward.
6. **Window states:** test normal, maximized, minimized/restored, non-resizable and minimum-size windows. Native maximize should behave like the window's maximize button. Undo should restore the previous frame and maximized state. Close an arranged app and try undo.
7. **Displays:** test one monitor; 100%/150% mixed scaling; a monitor left of the primary with negative coordinates; portrait display; and taskbars on different edges. Ring size, previews, gaps and final positions should remain aligned. Test Tab and Shift+Tab with the same selected layout.
8. **Windows integration:** test Start-menu install, Installed apps registration, opt-in sign-in startup, update after quitting, pause/resume, Explorer/taskbar restart, administrator restart, and uninstall. Uninstall must remove only Orbit's installed files and integration, preserving settings.
9. **Permissions and compatibility:** test ordinary Notepad, Explorer, Edge/Chrome and an Electron app, then an administrator app. Unelevated Orbit should explain the permission mismatch. Secure desktops, protected apps, custom chrome and fullscreen apps are compatibility boundaries until tested.
10. **Resources:** observe CPU while idle in Task Manager, then while selecting layouts. Record working set, first-open latency and subsequent gesture latency; no performance figures have been established. Check for GDI handle growth after repeated opens and display changes.

Release acceptance requires passing these checks on Windows; host-side unit tests and static PE checks are separate evidence.

## Gift installation acceptance (pending)

- Open the unsigned x64 gift installer on a clean Windows account. Check Hebrew text, PNG transparency and the handwritten “באהבה, מספי” dedication.
- Cancel a partial tear, change focus, resume it, and use the keyboard alternative. Opening the wrapping alone must not write installation files.
- Confirm the two optional choices default to off on a fresh install. Install without administrator permission or network connectivity.
- Check the installed app has its own manifest and launches after the final button; the bootstrap must then exit. Test both optional shortcuts and sign-in startup with redirected Desktop/Start folders.
- Check Start and Installed apps entries, preserved preferences on reinstall, an informative failure when an old app is running, and uninstall cleanup of Start/Desktop shortcuts.
- Run the included Check-Windows-Startup.cmd, then complete the real window-layout checks above. Browser preview interaction is a simulation and does not establish these results.
