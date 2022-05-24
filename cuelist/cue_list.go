package cuelist

// CueList stores a list of cues and can play them back
type CueList struct {
	// tracking
	State State

	// TODO - make CueList thread safe one day.
	// lock   sync.Mutex
}

func (cl *CueList) Initialize() {
	// TODO - create a deep copy of the current fixture state and store it internally on the cue list for tracking.
}

// Go plays the next cue
func (cl *CueList) Go() bool {
	return true
}

// Render is called each time a new frame is requested
func (cl *CueList) Render() {

}

// State is the basic properties of the cuelist
type State struct {
	CurrentPercent float64
	CurrentBytes   float64
	SecondsSince   float64
	SecondsLeft    float64
	KBsPerSecond   float64
}
