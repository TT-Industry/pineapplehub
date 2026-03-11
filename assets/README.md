# Assets

Static files embedded into the WASM binary at compile time via `include_bytes!`.

| File | Purpose | Source |
|------|---------|--------|
| `favicon.png` | App icon shown in title bar and browser tab | Custom design |
| `github-mark.png` | GitHub logo button (RGBA, transparent bg, 64×64) | [GitHub Logos](https://github.com/logos) |
| `material-symbols.ttf` | Icon font subset (Material Symbols Outlined) | See `src/icons.rs` for regeneration steps |
| `NotoSansSC-Regular.ttf` | CJK text rendering (Simplified Chinese) | [Google Fonts](https://fonts.google.com/noto/specimen/Noto+Sans+SC) |
| `_headers` | HTTP headers for Trunk dev server (COOP/COEP for SharedArrayBuffer) | Manual |

## Notes

- **`material-symbols.ttf`** is a pyftsubset-generated subset (~25 KB) of the
  full Material Symbols Outlined variable font (~10 MB). Regenerate with:
  ```sh
  # 1. Download full font
  curl -sL -o /tmp/MaterialSymbolsOutlined.ttf \
    "https://github.com/google/material-design-icons/raw/master/variablefont/MaterialSymbolsOutlined%5BFILL%2CGRAD%2Copsz%2Cwght%5D.ttf"

  # 2. Look up new codepoints
  curl -sL "https://raw.githubusercontent.com/google/material-design-icons/master/variablefont/MaterialSymbolsOutlined%5BFILL%2CGRAD%2Copsz%2Cwght%5D.codepoints" \
    | grep -E '^(icon_name) '

  # 3. Generate subset (update --unicodes when adding icons)
  pyftsubset /tmp/MaterialSymbolsOutlined.ttf \
    --unicodes="U+E000,U+E0CB,U+E162,U+E166,U+E26B,U+E5CB,U+E5CC,U+E5CD,U+E5D4,U+E5D7,U+E5D8,U+E5DB,U+E627,U+E873,U+E888,U+E88B,U+E88E,U+E8B3,U+E8B6,U+E8FD,U+E92E,U+EA5B,U+F083,U+F090,U+F097,U+F09A,U+F0BE,U+F0FF,U+F18B" \
    --output-file=assets/material-symbols.ttf \
    --layout-features="" --no-hinting --desubroutinize
  ```
  Then add the corresponding `ICON_*` constant in `src/icons.rs`.

- **`NotoSansSC-Regular.ttf`** is a pyftsubset-generated subset (~7 MB) of the
  full Noto Sans SC variable font (~25 MB). It covers ASCII, CJK punctuation,
  and CJK Unified Ideographs (U+4E00–9FFF). Regenerate with:
  ```sh
  pyftsubset NotoSansSC-full.ttf \
    --output-file=assets/NotoSansSC-Regular.ttf \
    --unicodes="U+0000-007F,U+2000-206F,U+3000-303F,U+4E00-9FFF,U+FF01-FF5E" \
    --no-hinting --desubroutinize \
    --drop-tables=GPOS,GSUB,GDEF,STAT,fvar,gvar,avar,cvar,HVAR,MVAR
  ```
  Variable-font tables (fvar, gvar, etc.) must be dropped, or the WASM binary
  may crash with a blank page due to cosmic-text parsing failures.

- **`github-mark.png`** uses a transparent background so it blends with any
  theme. The original GitHub mark has a white background; transparency was
  applied via Pillow (`PIL`).
