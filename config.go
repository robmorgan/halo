package main

// HaloConfig represents options that configure the global behavior of the program
type HaloConfig struct {
	// Fixtures stores the patched fixtures
	Fixtures map[string]interface{}
}

// Create a new HaloConfig object with reasonable defaults for real usage
func NewHaloConfig() (*HaloConfig, error) {
	// TODO - support passing in a config file one day

	return &HaloConfig{
		Fixtures: make(map[string]interface{}),
	}, nil
}
