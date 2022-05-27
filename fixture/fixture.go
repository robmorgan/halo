package fixture

import "fmt"

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

func (f *Fixture) GetChannel(chType string) (Channel, error) {
	// try to find the correct channel type
	for key, ch := range f.Channels {
		fmt.Println("Key:", key, "=>", "Channel:", ch)
		if ch.Type == chType {
			return ch, nil
		}
	}

	// couldn't find the relevant channel
	return *&Channel{}, fmt.Errorf("could not find fixture channel of type: %s", chType)
}

func (f *Fixture) SetValue(chType string, value Value) error {
	ch, err := f.GetChannel(chType)
	if err != nil {
		return err
	}
	ch.SetValue(value)
	return nil
}

func (f *Fixture) GetValue(chType string) (Value, error) {
	ch, err := f.GetChannel(chType)
	if err != nil {
		return 0.0, err
	}

	return ch.Value, nil
}

func (f *Fixture) SetIntensity(intensity float64) {
	f.SetValue(TypeIntensity, Value(intensity))
	f.needsUpdate = true
}

func (f *Fixture) GetIntensity() float64 {
	return 0.0
}

func (f *Fixture) SetColor(color float64) {
	f.SetValue(TypeColorRed, Value(color))
	f.needsUpdate = true
}

func (f *Fixture) GetColor() (Value, error) {
	ch, err := f.GetChannel(TypeColorRed)
	if err != nil {
		return 0.0, err
	}
	return ch.Value, nil
}

func (f *Fixture) NeedsUpdate() bool {
	return f.needsUpdate
}
