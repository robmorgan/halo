#!/usr/bin/env bash
# Rebuild a tabular copy of a font in crates/halo/assets/fonts: its tabular
# figures (OpenType `tnum`) frozen into the cmap, renamed so it never
# collides with the stock face. egui applies no OpenType features, so this
# is how live numbers get fixed-width digits.
#
# Usage: scripts/freeze-tabular.sh path/to/Inter-SemiBold.ttf
#        scripts/freeze-tabular.sh path/to/BarlowCondensed-SemiBold.ttf
# Needs `pyftfeatfreeze` (pip install opentype-feature-freezer).
set -euo pipefail

src="${1:?usage: $0 path/to/Inter-SemiBold.ttf | path/to/BarlowCondensed-SemiBold.ttf}"
fonts="$(dirname "$0")/../crates/halo/assets/fonts"

case "$(basename "$src")" in
    Inter-SemiBold.ttf)
        out="$fonts/InterTabular-SemiBold.ttf"
        rename='Inter/Inter Tabular'
        ;;
    BarlowCondensed-SemiBold.ttf)
        out="$fonts/BarlowCondensedTabular-SemiBold.ttf"
        rename='Barlow Condensed/Barlow Condensed Tabular'
        ;;
    *)
        echo "don't know a tabular name for $(basename "$src")" >&2
        exit 1
        ;;
esac

pyftfeatfreeze -f tnum -R "$rename" "$src" "$out"
echo "wrote $out"
