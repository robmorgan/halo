#!/usr/bin/env bash
# Rebuild crates/halo/assets/fonts/InterTabular-SemiBold.ttf: Inter SemiBold
# with tabular figures (OpenType `tnum`) frozen into its cmap, renamed so it
# never collides with the stock face. egui applies no OpenType features, so
# this is how live numbers get fixed-width digits.
#
# Usage: scripts/freeze-tabular.sh path/to/Inter-SemiBold.ttf
# Needs `pyftfeatfreeze` (pip install opentype-feature-freezer).
set -euo pipefail

src="${1:?usage: $0 path/to/Inter-SemiBold.ttf}"
out="$(dirname "$0")/../crates/halo/assets/fonts/InterTabular-SemiBold.ttf"

pyftfeatfreeze -f tnum -R 'Inter/Inter Tabular' "$src" "$out"
echo "wrote $out"
