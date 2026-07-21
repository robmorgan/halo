# Preset System

The preset system in Halo allows you to define reusable lighting states that can be referenced in cues, making show creation faster and more consistent.

## Preset Types

Halo supports five types of presets:

1. **Color Presets** - RGB, RGBW, color wheels
2. **Position Presets** - Pan and Tilt values
3. **Intensity Presets** - Dimmer values
4. **Beam Presets** - Focus, zoom, gobo, strobe, etc.
5. **Effect Presets** - Reusable effect configurations

## Fixture Groups

Presets target fixture groups rather than individual fixtures. This allows you to apply the same preset to different sets of fixtures.

### Creating Fixture Groups

```rust
use halo_core::FixtureGroup;

let spots = FixtureGroup {
    id: 1,
    name: "All Spots".to_string(),
    fixture_ids: vec![5, 6],
};

let beams = FixtureGroup {
    id: 2,
    name: "Beam Fixtures".to_string(),
    fixture_ids: vec![1, 2],
};
```

## Creating Presets

### Color Preset

```rust
use halo_core::{ColorPreset, Preset};
use halo_fixtures::ChannelType;

let mut warm_white = ColorPreset::new(1, "Warm White".to_string(), vec![1]); // targets group 1
warm_white.add_value(ChannelType::Red, 255);
warm_white.add_value(ChannelType::Green, 200);
warm_white.add_value(ChannelType::Blue, 150);
warm_white.add_value(ChannelType::White, 100);
```

### Position Preset

```rust
use halo_core::{PositionPreset, Preset};

let dj_position = PositionPreset::new(1, "DJ Position".to_string(), vec![1])
    .with_pan(208)
    .with_tilt(130);
```

### Intensity Preset

```rust
use halo_core::{IntensityPreset, Preset};

let full = IntensityPreset::new(1, "Full".to_string(), vec![1, 2], 255);
let half = IntensityPreset::new(2, "50%".to_string(), vec![1, 2], 128);
```

### Beam Preset

```rust
use halo_core::{BeamPreset, Preset};
use halo_fixtures::ChannelType;

let mut tight_beam = BeamPreset::new(1, "Tight Beam".to_string(), vec![1]);
tight_beam.add_value(ChannelType::Zoom, 0);
tight_beam.add_value(ChannelType::Focus, 128);
```

## Using Presets in Cues

### Basic Preset Reference

```rust
use halo_core::{PresetReference, PresetType};

// Reference a color preset in a cue
let preset_ref = PresetReference {
    preset_id: 1,
    preset_type: PresetType::Color,
    fixture_group_id: None, // Apply to all groups the preset targets
    overrides: vec![],
};
```

### Preset Reference with Overrides

```rust
use halo_core::{PresetReference, PresetType, StaticValue};
use halo_fixtures::ChannelType;

// Reference a color preset but override red for fixture 5
let preset_ref = PresetReference {
    preset_id: 1,
    preset_type: PresetType::Color,
    fixture_group_id: Some(1),
    overrides: vec![
        StaticValue {
            fixture_id: 5,
            channel_type: ChannelType::Red,
            value: 200, // Override from 255 to 200
        }
    ],
};
```

## Adding Presets to a Show

```rust
use halo_core::{Show, PresetLibrary, Preset};

let mut show = Show::new("My Show".to_string());

// Add fixture groups
show.fixture_groups.push(spots);
show.fixture_groups.push(beams);

// Add presets
show.presets.add_preset(Preset::Color(warm_white));
show.presets.add_preset(Preset::Position(dj_position));
show.presets.add_preset(Preset::Intensity(full));
```

## Working with the Programmer

### Creating Presets from Programmer

```rust
use halo_core::Programmer;

let mut programmer = Programmer::new();

// Set some values in the programmer
programmer.add_value(5, ChannelType::Red, 255);
programmer.add_value(5, ChannelType::Green, 100);
programmer.add_value(5, ChannelType::Blue, 50);

// Create a preset from current programmer state
if let Some(preset) = programmer.create_color_preset(1, "My Color".to_string(), vec![1]) {
    show.presets.add_preset(Preset::Color(preset));
}
```

### Applying Presets to Programmer

```rust
// Get a preset from the library
if let Some(preset) = show.presets.get_preset(&PresetType::Color, 1) {
    // Apply it to specific fixtures
    programmer.apply_preset(&preset, &[5, 6]);
}
```

## Resolving Cues with Presets

The `CueResolver` expands preset references into concrete DMX values:

```rust
use halo_core::{CueResolver, Cue};

let resolver = CueResolver::new(&show.presets, &show.fixture_groups);

// Resolve a cue with preset references
let resolved = resolver.resolve_cue(&cue);

// Access the final static values
for value in &resolved.static_values {
    println!("Fixture {}: {} = {}", value.fixture_id, value.channel_type, value.value);
}
```

## JSON Schema Example

Here's how presets appear in a show file:

```json
{
  "name": "My Show",
  "fixture_groups": [
    {
      "id": 1,
      "name": "All Spots",
      "fixture_ids": [5, 6]
    }
  ],
  "presets": {
    "color": [
      {
        "id": 1,
        "name": "Warm White",
        "fixture_groups": [1],
        "values": [
          { "channel_type": "Red", "value": 255 },
          { "channel_type": "Green", "value": 200 },
          { "channel_type": "Blue", "value": 150 }
        ]
      }
    ],
    "position": [
      {
        "id": 1,
        "name": "DJ Position",
        "fixture_groups": [1],
        "pan": 208,
        "tilt": 130
      }
    ],
    "intensity": [
      {
        "id": 1,
        "name": "Full",
        "fixture_groups": [1],
        "dimmer": 255
      }
    ]
  },
  "cue_lists": [
    {
      "name": "Main",
      "cues": [
        {
          "id": 1,
          "name": "Warm Look",
          "fade_time": { "secs": 3, "nanos": 0 },
          "preset_references": [
            {
              "preset_id": 1,
              "preset_type": "Color",
              "fixture_group_id": null,
              "overrides": []
            },
            {
              "preset_id": 1,
              "preset_type": "Intensity",
              "fixture_group_id": null,
              "overrides": []
            }
          ],
          "static_values": [],
          "effects": [],
          "pixel_effects": [],
          "timecode": null,
          "is_blocking": false
        }
      ],
      "audio_file": null
    }
  ]
}
```

## Benefits

1. **Rapid Show Creation** - Define common looks once, reuse everywhere
2. **Consistency** - Same color/position across all uses
3. **Easy Updates** - Change a preset, all cues using it update (unless overridden)
4. **Flexibility** - Override specific values per cue when needed
5. **Professional Workflow** - Matches industry-standard console behavior (ETC EOS, MA)

## Backward Compatibility

The preset system is fully backward compatible:
- Existing show files without presets will load normally
- The `preset_references` field in cues defaults to an empty vector
- The `fixture_groups` and `presets` fields in shows default to empty

You can gradually migrate existing cues to use presets without breaking existing functionality.

