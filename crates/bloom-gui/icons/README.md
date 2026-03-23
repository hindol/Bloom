# Bloom App Icons

Place icon files here for packaging:

| File | Format | Used for |
|------|--------|----------|
| `icon.icns` | Apple Icon Image | macOS .app bundle |
| `icon.ico` | Windows Icon | Windows .exe and installer |
| `icon.png` | PNG 1024×1024 | Linux desktop, README, docs site |
| `icon_32.png` | PNG 32×32 | Window title bar (winit) |

## Generating icons

From a 1024×1024 PNG source (`icon.png`):

```bash
# macOS .icns (requires iconutil)
mkdir icon.iconset
sips -z 16 16 icon.png --out icon.iconset/icon_16x16.png
sips -z 32 32 icon.png --out icon.iconset/icon_16x16@2x.png
sips -z 32 32 icon.png --out icon.iconset/icon_32x32.png
sips -z 64 64 icon.png --out icon.iconset/icon_32x32@2x.png
sips -z 128 128 icon.png --out icon.iconset/icon_128x128.png
sips -z 256 256 icon.png --out icon.iconset/icon_128x128@2x.png
sips -z 256 256 icon.png --out icon.iconset/icon_256x256.png
sips -z 512 512 icon.png --out icon.iconset/icon_256x256@2x.png
sips -z 512 512 icon.png --out icon.iconset/icon_512x512.png
sips -z 1024 1024 icon.png --out icon.iconset/icon_512x512@2x.png
iconutil -c icns icon.iconset

# Windows .ico (requires ImageMagick)
convert icon.png -resize 256x256 icon.ico

# Title bar icon
sips -z 32 32 icon.png --out icon_32.png
```

## Design: Style 1A — Green seedling on dark background

- Simple two-leaf sprout, flat design
- Green: #5EBC52 (Bloom's accent_green)
- Background: #141414 (Bloom's background) or transparent
- Must read clearly at 16×16
