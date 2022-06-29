package fixture

import (
	"fmt"

	"github.com/lucasb-eyer/go-colorful"
	"github.com/robmorgan/halo/logger"
)

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
	Channels map[int]*Channel

	/// State

	// Intensity
	intensity float64

	// Color
	color colorful.Color

	// Does the renderer need to update the fixture
	needsUpdate bool
}

// Create a new Fixture object with reasonable defaults for real usage.
func NewFixture(id int, address int, mode int, channels map[int]*Channel) *Fixture {
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

func (f *Fixture) GetChannel(chType string) (*Channel, error) {
	// try to find the correct channel type
	for _, ch := range f.Channels {
		if ch.Type == chType {
			return ch, nil
		}
	}

	// couldn't find the relevant channel
	return nil, fmt.Errorf("could not find fixture channel of type: %s", chType)
}

func (f *Fixture) SetIntensity(intensity float64) {
	f.intensity = intensity
	f.needsUpdate = true
}

func (f *Fixture) GetIntensity() float64 {
	return f.intensity
}

func (f *Fixture) SetColor(c colorful.Color) {
	f.color = c
	f.needsUpdate = true
}

func (f *Fixture) GetColor() colorful.Color {
	return f.color
}

func (f *Fixture) SetColorFromHex(s string) {
	c, err := colorful.Hex(s)
	if err != nil {
		logger := logger.GetProjectLogger()
		logger.Debugf("error getting RGB from string: %s, %v", s, err)
	}
	f.color = c
}

func (f *Fixture) NeedsUpdate() bool {
	return f.needsUpdate
}

func (f *Fixture) HasUpdated() {
	f.needsUpdate = false
}
