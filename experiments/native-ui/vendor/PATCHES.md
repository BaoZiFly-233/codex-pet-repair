# Slint 1.17.1 Windows backend patch

Source: crates.io i-slint-backend-winit 1.17.1, copied from the Cargo registry without changing its licensing.

2026-09-07 refactor review: all patches below remain pinned and unchanged. The application now uses labelled standard Switch widgets and layout containers; no additional widget input or rendering patch was introduced. Development preview and PNG export are opt-in. Do not remove indirect image dependencies or restore partial Windows frame submission as a size optimization.

The theme guard change adds `muda` to the Windows cfg guard in `WinitWindowAdapter::set_color_scheme`. Upstream references the conditional `muda_adapter` field even when the muda feature is disabled, producing E0026/E0282 on Windows. This patch makes the theme update conditional on that feature, matching the field definition.

The app selects the backend directly without muda so ContextMenuArea uses Slint's Fluent menu implementation. Recheck and remove this patch when upgrading to a version that fixes the guard. Vendor source is for reproducible builds; it is not copied into the binary distribution. The package includes the selected Slint royalty-free license.

## Opaque Windows software-rendered main window

In renderer/sw.rs::resume, decorated Windows windows use with_transparent(false). The app main window is fully opaque; enabling platform transparency was unnecessary. This alone did not fix missing pixels: the next user screenshot showed black outer regions, so it must not be treated as the complete rendering fix.

## Complete Windows frame submission

In renderer/sw.rs::render, Windows uses RepaintBufferType::NewBuffer on every requested frame and softbuffer Buffer::present(), rather than presenting only the renderer damage rectangle. Non-Windows behavior is unchanged. The pinned softbuffer 0.4.8 Win32 implementation BitBlts only the supplied damage rectangles but then calls ValidateRect(hwnd, null), validating the whole window. Its age() is 1 once the retained bitmap has been presented; it does not describe which on-screen pixels need repainting. An unchanged retained bitmap is not proof that the whole exposed window is up to date.

The full-frame path avoids both stale incremental regions and incomplete submission. It adds no idle timer or runtime dependency, but does more pixel work per requested frame. Static Window::take_snapshot output bypasses this presentation failure and cannot certify that the OS window was displayed correctly; live exposure/resize/state-change validation remains necessary.

## Popup menu typography

Source: crates.io i-slint-compiler 1.17.1. Add `default-font-size: 16px` to `widgets/common/menus.slint`'s PopupMenuImpl window. This internal menu window does not expose its font size through ContextMenuArea. The compiler is a build dependency; vendor source and compiler binaries are not included in the distribution. Remove this patch if Slint adds an application-level popup typography setting.

## Explicit light blue accent

In widgets/fluent/styling.slint, the light theme uses the original Fluent default accent #005FB8 and selection #0078D4 directly, skipping accentify()'s system-color substitution. This is the user's explicit blue/white requirement. Dark theme accent behavior is unchanged. Hover, pressed, disabled, gradients and other widget styling still come from the standard implementation; disabled buttons remain disabled-looking rather than being falsely presented as active.

## Fluent popup semantic strokes

In widgets/fluent/menu.slint, MenuFrame uses FluentPalette.control-background-stroke-flyout and MenuItem separators use FluentPalette.divider instead of the generic border brush. This applies the existing Fluent semantic tokens to both the standard context menu and the license popup's shared MenuFrame. Layout, opaque background, shadow, menu input and focus handling remain upstream. This is a deliberate application theme adjustment, not an unmodified upstream appearance.
