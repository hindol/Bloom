# Bloom App Icons

All icons have **transparent backgrounds** outside the rounded-rect shape.
Generated from the source artwork at project root (`Bloom Icon Light and Dark.png`).

## Files

### Master icons

| File | Size | Used for |
|------|------|----------|
| `icon.png` | 1024×1024 | Master dark icon (green seedling on dark rounded square) |
| `icon_light.png` | 1024×1024 | Light variant (seedling only, for README/docs on light backgrounds) |

### Platform bundles

| File | Sizes included | Used for |
|------|----------------|----------|
| `icon.icns` | 16–1024 + @2x | macOS .app bundle |
| `icon.ico` | 16, 24, 32, 48, 64, 128, 256 | Windows .exe and installer |

### Individual PNGs

| File | Used for |
|------|----------|
| `icon_16x16.png` | Taskbar, small icon contexts |
| `icon_22x22.png` | Linux system tray |
| `icon_24x24.png` | Linux toolbar |
| `icon_32x32.png` | Window title bar (winit), Linux panel |
| `icon_48x48.png` | Linux app launcher |
| `icon_64x64.png` | Dock/panel on HiDPI |
| `icon_128x128.png` | macOS Finder, app stores |
| `icon_256x256.png` | HiDPI app grids |
| `icon_512x512.png` | macOS Retina, web |

## Regenerating

All icons are generated via a Python script (requires Pillow + numpy):

```bash
# From project root — see the generation script in session history
python3 scripts/generate_icons.py

# macOS .icns specifically (requires iconutil from Xcode CLI tools)
iconutil -c icns icon.iconset -o icon.icns
```

## Design

- Green seedling on dark rounded square (dark variant)
- Seedling only on transparent background (light variant)
- Rounded corners with anti-aliased edges
- Must read clearly at 16×16
