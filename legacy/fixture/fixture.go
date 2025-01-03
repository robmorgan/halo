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

// Fixture represents is the concrete implementation of a lighting fixture.
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

// State represents the current state of the fixture
type State struct {
	// intensity
	Intensity int

	// color
	RGB    utils.RGB
	Strobe int

	// Movement
	Pan  int
	Tilt int
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

// GetID returns the a unique id: dmx address info + profile name
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

func (f *Fixture) SetState(manager Manager, target TargetState) {
	f.setIntensityToStateAndDMX(manager, target.Intensity)
	f.blindlySetRGBToStateAndDMX(manager, target.RGB)
	f.setPositionToStateAndDMX(manager, target.Pan, target.Tilt)
	manager.SetState(f.Name, target.ToState())
}

// This is the old function from the frame-based version of Halo. You've kept this because it has some logic
// concerned with figuring out a color fade from one state to another. You no longer need to handle interpolated fades
// here. Instead you blindly set the target state and let the renderer handle the interpolation.
//
// SetState updates the fixture's state.
// TODO: other properties? on/off?
// func (f *Fixture) SetState(manager Manager, target TargetState) {
// 	currentState := manager.GetState(f.Name)
// 	numSteps := int(target.Duration / tickIntervalFadeInterpolation)

// 	logger := logger.GetProjectLogger()
// 	logger.Printf("dmx fade [%s] to [%s] over %d steps", currentState.RGB.TermString(), target.String(), numSteps)

// 	for x := 0; x < numSteps; x++ {
// 		intVal := utils.GetDimmerFadeValue(target.Intensity, x, numSteps)
// 		panVal := utils.GetDimmerFadeValue(target.Pan, x, numSteps)
// 		tiltVal := utils.GetDimmerFadeValue(target.Tilt, x, numSteps)
// 		interpolated := currentState.RGB.GetInterpolatedFade(target.RGB, x, numSteps)

// 		// keep state updated
// 		f.setIntensityToStateAndDMX(manager, intVal)
// 		f.setStrobeToStateAndDMX(manager, target.Strobe)
// 		f.blindlySetRGBToStateAndDMX(manager, interpolated)
// 		f.setPositionToStateAndDMX(manager, panVal, tiltVal)

// 		time.Sleep(tickIntervalFadeInterpolation)
// 	}

// 	f.setIntensityToStateAndDMX(manager, target.Intensity)
// 	f.blindlySetRGBToStateAndDMX(manager, target.RGB)
// 	f.setPositionToStateAndDMX(manager, target.Pan, target.Tilt)
// 	manager.SetState(f.Name, target.ToState())

// }

func (f *Fixture) setIntensityToStateAndDMX(manager Manager, value int) {
	intChannelID := f.getChannelIDForAttributes(profile.ChannelTypeIntensity)
	manager.SetDMXState(dmxOperation{universe: f.Universe, channel: intChannelID[0], value: value})
}

func (f *Fixture) setStrobeToStateAndDMX(manager Manager, value int) {
	strobeChannelID := f.getChannelIDForAttributes(profile.ChannelTypeStrobe)
	manager.SetDMXState(dmxOperation{universe: f.Universe, channel: strobeChannelID[0], value: value})
}

func (f *Fixture) setPositionToStateAndDMX(manager Manager, pan int, tilt int) {
	channelIDs := f.getChannelIDForAttributes(profile.ChannelTypePan, profile.ChannelTypeTilt)

	manager.SetDMXState(dmxOperation{universe: f.Universe, channel: channelIDs[0], value: pan},
		dmxOperation{universe: f.Universe, channel: channelIDs[1], value: tilt})
}

// for a given color, blindly set the r, g and b channels to that color, and update the state to reflect.
func (f *Fixture) blindlySetRGBToStateAndDMX(manager Manager, color utils.RGB) {
	rgbChannelIds := f.getChannelIDForAttributes(profile.ChannelTypeRed, profile.ChannelTypeGreen, profile.ChannelTypeBlue)
	rVal, gVal, bVal := color.AsComponents()

	manager.SetDMXState(dmxOperation{universe: f.Universe, channel: rgbChannelIds[0], value: rVal},
		dmxOperation{universe: f.Universe, channel: rgbChannelIds[1], value: gVal},
		dmxOperation{universe: f.Universe, channel: rgbChannelIds[2], value: bVal})

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
