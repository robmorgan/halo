#!/bin/bash
# Downloads GiantSteps tempo dataset for BPM accuracy testing
# Audio files are stored in the repo but ignored by .gitignore
#
# Usage: ./scripts/download_test_audio.sh
#
# This script:
# 1. Clones the GiantSteps tempo dataset repository into the fixtures directory
# 2. Downloads the audio files (~1GB) using the dataset's own script
# 3. Generates ground_truth.json from annotations
# 4. Copies audio files to the test fixtures audio directory

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
FIXTURES_DIR="$REPO_ROOT/crates/dj/tests/fixtures"
AUDIO_DIR="$FIXTURES_DIR/audio"
DATASET_DIR="$FIXTURES_DIR/giantsteps-tempo-dataset"

echo "=== GiantSteps Tempo Dataset Download Script ==="
echo ""
echo "Fixtures directory: $FIXTURES_DIR"
echo "Dataset directory:  $DATASET_DIR"
echo "Audio directory:    $AUDIO_DIR"
echo ""

# Check for required tools
command -v curl >/dev/null 2>&1 || { echo "Error: curl is required"; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "Error: python3 is required"; exit 1; }
command -v git >/dev/null 2>&1 || { echo "Error: git is required"; exit 1; }

# Clone or update the dataset repository
if [ -d "$DATASET_DIR" ]; then
    echo "Updating existing GiantSteps repository..."
    cd "$DATASET_DIR"
    git pull
else
    echo "Cloning GiantSteps tempo dataset..."
    git clone https://github.com/GiantSteps/giantsteps-tempo-dataset.git "$DATASET_DIR"
fi

# Create audio directory
mkdir -p "$AUDIO_DIR"

# Download audio files if not already present
AUDIO_COUNT=$(find "$AUDIO_DIR" -name "*.mp3" 2>/dev/null | wc -l | tr -d ' ')
if [ "$AUDIO_COUNT" -lt 600 ]; then
    echo ""
    echo "Downloading audio files (this may take a while, ~1GB)..."
    echo ""

    cd "$DATASET_DIR"

    # Use the dataset's own download script, but download to our audio directory
    # We'll iterate over the md5 files ourselves for better control

    BASEURL="https://www.cp.jku.at/datasets/giantsteps/backup/"
    BACKUPURL="http://geo-samples.beatport.com/lofi/"

    TOTAL=$(ls -1 md5/*.md5 2>/dev/null | wc -l | tr -d ' ')
    COUNT=0
    ERRORS=0

    for md5file in md5/*.md5; do
        COUNT=$((COUNT + 1))

        # Get filename from md5 file
        basename_md5=$(basename "$md5file")
        mp3filename="${basename_md5%.md5}.mp3"
        target_file="$AUDIO_DIR/$mp3filename"

        # Skip if already downloaded
        if [ -f "$target_file" ]; then
            printf "\r[$COUNT/$TOTAL] Skipping $mp3filename (exists)              "
            continue
        fi

        printf "\r[$COUNT/$TOTAL] Downloading $mp3filename...                    "

        # Try primary URL first
        if curl -s -f -o "$target_file" "${BASEURL}${mp3filename}" 2>/dev/null; then
            : # Success
        elif curl -s -f -o "$target_file" "${BACKUPURL}${mp3filename}" 2>/dev/null; then
            : # Success from backup
        else
            ERRORS=$((ERRORS + 1))
            rm -f "$target_file" 2>/dev/null
        fi
    done

    echo ""
    echo ""
    echo "Download complete! Errors: $ERRORS"
else
    echo "Audio files already downloaded ($AUDIO_COUNT files found)"
fi

# Generate ground_truth.json from annotations
echo ""
echo "Generating ground_truth.json from annotations..."

FIXTURES_DIR="$FIXTURES_DIR" DATASET_DIR="$DATASET_DIR" python3 << 'PYTHON_SCRIPT'
import json
import os
from pathlib import Path

FIXTURES_DIR = os.environ.get("FIXTURES_DIR")
DATASET_DIR = os.environ.get("DATASET_DIR")

# Parse annotations from the dataset
annotations_dir = Path(DATASET_DIR) / "annotations" / "tempo"
annotations_v2_dir = Path(DATASET_DIR) / "annotations_v2" / "tempo"
genre_dir = Path(DATASET_DIR) / "annotations" / "genre"

tracks = []

# Prefer v2 annotations (crowdsourced corrections), fallback to v1
for anno_file in sorted(annotations_dir.glob("*.bpm")):
    filename = anno_file.stem  # e.g., "1003173.LOFI"

    # Read BPM from annotation (format: single float value)
    try:
        bpm = float(anno_file.read_text().strip())
    except:
        continue

    # Check for v2 annotation (crowdsourced correction)
    v2_file = annotations_v2_dir / f"{filename}.bpm"
    if v2_file.exists():
        try:
            bpm = float(v2_file.read_text().strip())
        except:
            pass

    # Read genre if available
    genre = "unknown"
    genre_file = genre_dir / f"{filename}.genre"
    if genre_file.exists():
        try:
            genre = genre_file.read_text().strip().lower().replace(" ", "-")
        except:
            pass

    tracks.append({
        "filename": f"{filename}.mp3",
        "expected_bpm": round(bpm, 2),
        "tolerance_percent": 2.0,
        "genre": genre,
        "notes": ""
    })

# Create ground truth JSON
ground_truth = {
    "version": 1,
    "dataset": "giantsteps-tempo",
    "description": "BPM ground truth from GiantSteps Tempo Dataset (crowdsourced corrections)",
    "source": "https://github.com/GiantSteps/giantsteps-tempo-dataset",
    "tracks": tracks
}

output_path = Path(FIXTURES_DIR) / "ground_truth.json"
with open(output_path, "w") as f:
    json.dump(ground_truth, f, indent=2)

print(f"Generated ground_truth.json with {len(tracks)} tracks")
PYTHON_SCRIPT

# Verify audio file count
FINAL_COUNT=$(find "$AUDIO_DIR" -name "*.mp3" 2>/dev/null | wc -l | tr -d ' ')

echo ""
echo "=== Download Complete ==="
echo ""
echo "Audio files:   $AUDIO_DIR ($FINAL_COUNT files)"
echo "Ground truth:  $FIXTURES_DIR/ground_truth.json"
echo "Dataset repo:  $DATASET_DIR"
echo ""
echo "Note: Audio files are ignored by .gitignore and won't be committed."
echo ""
echo "To run accuracy tests:"
echo "  cargo test --package halo-dj --features accuracy-tests -- --nocapture"
