package cuelist

import "github.com/robmorgan/halo/logger"

// CueList stores a list of cues and can play them back
type CueList struct {
	Name string

	// tracking
	State State

	Cues []*Cue
	// TODO - make CueList thread safe one day.
	// lock   sync.Mutex
}

func NewCueList(cueListName string) *CueList {
	logger := logger.GetProjectLogger()
	logger.Debugf("Cue list created with name: %s", cueListName)

	return &CueList{
		Name: cueListName,
		Cues: make([]*Cue, 0),
	}
}

func (cl *CueList) Initialize() {
	// TODO - create a deep copy of the current fixture state and store it internally on the cue list for tracking.
}

func (cl *CueList) NewCue(cueName string, cueInitializer func()) {
	logger := logger.GetProjectLogger()
	logger.Debugf("Cue created with name: %s", cueName)

	cue := &Cue{
		cueInitializerFunc: cueInitializer,
	}
	cl.Cues = append(cl.Cues, cue)
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
