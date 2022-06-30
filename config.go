package main

import (
	"github.com/robmorgan/halo/fixture"
	"github.com/sirupsen/logrus"
)

// HaloConfig represents options that configure the global behavior of the program
type HaloConfig struct {
	// Project logger
	Logger *logrus.Logger

	// The fixture profiles
	FixtureProfiles map[string]fixture.Profile

	// PatchedFixtures stores all of the patched fixtures in a custom struct
	PatchedFixtures *PatchedFixtures
}

// Create a new HaloConfig object with reasonable defaults for real usage
func NewHaloConfig() (*HaloConfig, error) {
	// TODO - support passing in a config file one day

	profiles := initializeFixtureProfiles()

	return &HaloConfig{
		FixtureProfiles: profiles,
		PatchedFixtures: PatchFixtures(),
	}, nil
}

func initializeFixtureProfiles() map[string]fixture.Profile {
	out := map[string]fixture.Profile{
		"shehds-par": {
			Name: "Shehds PAR",
			Channels: map[int]string{
				1: fixture.TypeIntensity,
				2: fixture.TypeColorRed,
				3: fixture.TypeColorGreen,
				4: fixture.TypeColorBlue,
			},
		},
	}

	return out
}
