package fixture

// Interface represents the set of methods required for a complete lighting fixture.
type Interface interface {
	NeedsUpdate() bool

	// Reset is called to reset the state of the fixture.
	Reset() error

	// Stop is called when the fixture should halt any in-flight actions.
	Stop() error
}

// MovingFixture is an optional interface that allows a fixture to enable pan/tilt functionality.
type MovingFixture interface {
	SetPan() error
	SetTilt() error
}

// Fixture is the ...TODO...
type Fixture struct {
	// Internal ID
	Id int

	// The DMX starting address
	Address int

	// The fixture channels
	Channels map[int]FixtureChannel

	// The number of channels the fixture uses
	Mode int
}

// Go plays the next cue
func (f *Fixture) GetChannelCount() int {
	return len(f.Channels)
}
