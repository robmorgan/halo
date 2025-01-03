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
