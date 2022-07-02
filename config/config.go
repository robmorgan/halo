package config

import (
	"github.com/robmorgan/halo/profile"
	"github.com/sirupsen/logrus"
)

// GetHaloConfig returns the current configuration
func GetHaloConfig() HaloConfig {
	val, _ := NewHaloConfig()
	return val
}

// HaloConfig represents options that configure the global behavior of the program
type HaloConfig struct {
	// Project logger
	Logger *logrus.Logger

	// The fixture profiles
	FixtureProfiles map[string]profile.Profile

	// PatchedFixtures stores all of the patched fixtures in a custom struct
	PatchedFixtures []PatchedFixture
}

// Create a new HaloConfig object with reasonable defaults for real usage
func NewHaloConfig() (HaloConfig, error) {
	// TODO - support passing in a config file one day

	profiles := initializeFixtureProfiles()

	return HaloConfig{
		FixtureProfiles: profiles,
		PatchedFixtures: PatchFixtures(),
	}, nil
}

func initializeFixtureProfiles() map[string]profile.Profile {
	out := map[string]profile.Profile{
		"shehds-par": {
			Name: "Shehds LED Flat PAR 12x3W RGBW",
			Channels: map[string]int{
				profile.ChannelTypeIntensity: 1,
				profile.ChannelTypeRed:       2,
				profile.ChannelTypeGreen:     3,
				profile.ChannelTypeBlue:      4,
			},
		},
		"shehds-led-bar-beam-8x12w": {
			Name: "Shehds LED Bar Beam 8x12W RGBW",
			// 9 channel mode
			Channels: map[string]int{
				profile.ChannelTypeMotorPosition:  1,
				profile.ChannelTypeMotorSpeed:     2,
				profile.ChannelTypeFunctionSelect: 3,
				profile.ChannelTypeFunctionSpeed:  4,
				profile.ChannelTypeIntensity:      5,
				profile.ChannelTypeRed:            6,
				profile.ChannelTypeGreen:          7,
				profile.ChannelTypeBlue:           8,
				profile.ChannelTypeWhite:          9,
			},
		},
	}

	return out
}
