package fixture

import (
	"fmt"
	"time"

	"github.com/lucasb-eyer/go-colorful"
	"github.com/robmorgan/halo/config"
	"github.com/robmorgan/halo/logger"
	"github.com/robmorgan/halo/profile"
	"github.com/robmorgan/halo/utils"
	"github.com/sirupsen/logrus"
)

// We are hard-coding this value for now, but it should be moved to config in the future.
var tickIntervalFadeInterpolation = time.Millisecond * 30

// Interface represents the set of methods required for a complete lighting fixture.
type Interface interface {
	// Clear is called to reset the state of the fixture.
	//Clear() error

	// Stop is called when the fixture should halt any in-flight actions.
	//Stop() error

	GetName() string
	GetID() string
	SetState(Manager, TargetState)
	NeedsUpdate() bool
}

// MovingFixtureInterface is an optional interface that allows a fixture to enable pan/tilt functionality.
type MovingFixtureInterface interface {
	SetPan() error
	SetTilt() error
}

// Fixture is the ...TODO...
type Fixture struct {
	// Internal ID
	Id int

	Name string

	// The DMX starting address
	Address int

	// The DMX universe
	Universe int

	// The number of channels the fixture uses
	Mode int

	// The fixture profile to use
	Profile string

	/// State

	// Intensity
	intensity float64

	// Color
	color colorful.Color

	// Does the renderer need to update the fixture
	needsUpdate bool
}

// TargetState represents the state of a fixture, is source of truth
type TargetState struct {
	// On   bool
	State
	Duration time.Duration // time to transition to the new state
}

// ToState converts a TargetState to a State
func (t *TargetState) ToState() State {
	return State{
		Intensity: t.Intensity,
		RGB:       t.RGB,
		Pan:       t.Pan,
		Tilt:      t.Tilt,
	}
}

func (t *TargetState) String() string {
	return fmt.Sprintf("Duration: %s, Intensity: %d, RGB: %s, Pan: %d, Tilt: %d", t.Duration, t.Intensity, t.RGB.TermString(), t.Pan, t.Tilt)
}

// State represents the current state of the fixture
type State struct {
	// intensity
	Intensity int

	// color
	RGB utils.RGB

	// Movement
	Pan  int
	Tilt int
}

// Create a new Fixture object with reasonable defaults for real usage.
func NewFixture(id int, address int, mode int, profile string) Fixture {
	return Fixture{
		Id:      id,
		Address: address,
		Mode:    mode,
		Profile: profile,
	}
}

// GetName returns the light's name.
func (f *Fixture) GetName() string {
	return f.Name
}

// GetID returns the a unique id: dmx address info + profile mame
func (f *Fixture) GetID() string {
	return fmt.Sprintf("u:%d-a:%d-p:%s", f.Universe, f.Address, f.Profile)
}

func (f *Fixture) getChannelIDForAttributes(attrs ...string) (ids []int) {
	profileMap := config.GetHaloConfig().FixtureProfiles
	profile, ok := profileMap[f.Profile]
	ids = make([]int, len(attrs))
	if ok {
		for x, attr := range attrs {
			channelIndex := getChannelIndexForAttribute(&profile, attr) //1 indexed
			ids[x] = f.Address + channelIndex - 1
		}
		return
	}
	logger := logger.GetProjectLogger()
	logger.WithFields(logrus.Fields{"fixture": f.Name}).Warn("could not find DMX profile")
	return
}

func getChannelIndexForAttribute(p *profile.Profile, attrName string) int {
	id, ok := p.Channels[attrName]
	if ok {
		return id
	}
	return 0
}

// SetState updates the fixture's state.
// TODO: other properties? on/off?
func (f *Fixture) SetState(manager Manager, target TargetState) {
	currentState := manager.GetState(f.Name)
	numSteps := int(target.Duration / tickIntervalFadeInterpolation)

	logger := logger.GetProjectLogger()
	logger.Printf("dmx fade [%s] to [%s] over %d steps", currentState.RGB.TermString(), target.String(), numSteps)

	for x := 0; x < numSteps; x++ {
		intVal := utils.GetDimmerFadeValue(target.Intensity, x, numSteps)
		interpolated := currentState.RGB.GetInterpolatedFade(target.RGB, x, numSteps)

		// keep state updated
		f.setIntensityToStateAndDMX(manager, intVal)
		f.blindlySetRGBToStateAndDMX(manager, interpolated)

		time.Sleep(tickIntervalFadeInterpolation)
	}

	f.setIntensityToStateAndDMX(manager, target.Intensity)
	f.blindlySetRGBToStateAndDMX(manager, target.RGB)
	manager.SetState(f.Name, target.ToState())

}

func (f *Fixture) setIntensityToStateAndDMX(manager Manager, value int) {
	intChannelID := f.getChannelIDForAttributes(profile.ChannelTypeIntensity)
	manager.SetDMXState(dmxOperation{universe: f.Universe, channel: intChannelID[0], value: value})
}

// for a given color, blindly set the r,g, and b channels to that color, and update the state to reflect
func (f *Fixture) blindlySetRGBToStateAndDMX(manager Manager, color utils.RGB) {
	rgbChannelIds := f.getChannelIDForAttributes(profile.ChannelTypeIntensity, profile.ChannelTypeRed, profile.ChannelTypeGreen, profile.ChannelTypeBlue)
	intVal := 200
	rVal, gVal, bVal := color.AsComponents()

	manager.SetDMXState(dmxOperation{universe: f.Universe, channel: rgbChannelIds[0], value: intVal},
		dmxOperation{universe: f.Universe, channel: rgbChannelIds[1], value: rVal},
		dmxOperation{universe: f.Universe, channel: rgbChannelIds[2], value: gVal},
		dmxOperation{universe: f.Universe, channel: rgbChannelIds[3], value: bVal})

	manager.SetState(f.Name, State{RGB: color})

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
