package cuelist

import (
	"sync"

	"github.com/robmorgan/halo/logger"
	"github.com/sirupsen/logrus"
)

// CueList stores a list of cues and can play them back
type CueList struct {
	Priority int
	Name     string

	// tracking
	//State State

	Cues          []Cue
	ProcessedCues []Cue
	ActiveCue     *Cue

	lock sync.Mutex
}

func (cl *CueList) deQueueNextCue() *Cue {
	cl.lock.Lock()
	defer cl.lock.Unlock()
	if len(cl.Cues) > 0 {
		x := cl.Cues[0]
		cl.Cues = cl.Cues[1:]
		return &x
	}
	return nil
}

// EnQueueCue puts a cue on the queue
// it also assigns the cue (and subcomponents) an ID
func (clm *Master) EnQueueCue(c Cue, cl *CueList) *Cue {
	cl.lock.Lock()
	defer cl.lock.Unlock()
	clm.AddIDsRecursively(&c)

	logger := logger.GetProjectLogger()
	logger.WithFields(logrus.Fields{"cue_id": c.ID, "stack_name": cl.Name}).Info("enqueued!")

	cl.Cues = append(cl.Cues, c)
	return &c
}

func NewCueList(cueListName string) *CueList {
	logger := logger.GetProjectLogger()
	logger.Debugf("Cue list created with name: %s", cueListName)

	return &CueList{
		Name:          cueListName,
		Cues:          make([]Cue, 0),
		ProcessedCues: make([]Cue, 0),
	}
}

func (cl *CueList) Initialize() {
	// TODO - create a deep copy of the current fixture state and store it internally on the cue list for tracking.
}

func (cl *CueList) NewCue(cueName string, cueInitializer func()) {
	logger := logger.GetProjectLogger()
	logger.Debugf("Cue created with name: %s", cueName)

	cue := Cue{
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
