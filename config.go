package main

import "github.com/robmorgan/halo/fixture"

// HaloConfig represents options that configure the global behavior of the program
type HaloConfig struct {
	// PatchedFixtures stores all of the patched fixtures in a fixture group
	PatchedFixtures *fixture.Group
}

// Create a new HaloConfig object with reasonable defaults for real usage
func NewHaloConfig() (*HaloConfig, error) {
	// TODO - support passing in a config file one day

	return &HaloConfig{
		PatchedFixtures: fixture.NewGroup(),
	}, nil
}
