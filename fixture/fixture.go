package fixture

// Interface represents the set of methods required for a complete lighting fixture.
type Interface interface {
	NeedsUpdate() bool

	// Clear is called to reset the state of the fixture.
	Clear() error

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

	// The number of channels the fixture uses
	Mode int

	// The fixture channels
	Channels map[int]Channel

	// The current fixture color
	Color int

	// Does the renderer need to update the fixture
	needsUpdate bool
}

// Create a new Fixture object with reasonable defaults for real usage.
func NewFixture(id int, address int, mode int, channels map[int]Channel) *Fixture {
	return &Fixture{
		Id:       id,
		Address:  address,
		Mode:     mode,
		Channels: channels,
	}
}

func (f *Fixture) GetChannelCount() int {
	return len(f.Channels)
}

func (f *Fixture) SetIntensity(intensity float64) {
	f.needsUpdate = true
}

func (f *Fixture) GetIntensity() float64 {
	return 0.0
}

func (f *Fixture) SetColor(color int) {
	f.Color = color
	f.needsUpdate = true
}

func (f *Fixture) GetColor() int {
	return f.Color
}

func (f *Fixture) NeedsUpdate() bool {
	return f.needsUpdate
}
