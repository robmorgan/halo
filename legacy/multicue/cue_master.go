package main

import (
	"time"

	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/rhythm"
)

// ProcessCueList(ctx context.Context, cl *CueList, wg *sync.WaitGroup)
// ProcessCue(c *Cue, wg *sync.WaitGroup)
// ProcessFrame(cf *Frame, wg *sync.WaitGroup)
// ProcessFrameAction(cfa *FrameAction, wg *sync.WaitGroup)
// EnQueueCue(c Cue, cl *CueList) *Cue
// AddIDsRecursively(c *Cue)
// GetDefaultCueList() *CueList
// ProcessForever(ctx context.Context, wg *sync.WaitGroup)
// GetFixtureManager() fixture.Manager

const MaxActiveCues = 5

type CueMaster struct {
	index         int
	metronome     *rhythm.Metronome
	pendingCues   []Cue
	activeCues    []Cue
	processedCues []Cue

	// effects
	//activeEffects []effect.Player
}

func NewCueMaster() *CueMaster {
	return &CueMaster{
		index:         0,
		metronome:     rhythm.NewMetronome(),
		pendingCues:   make([]Cue, 0),
		activeCues:    make([]Cue, 0),
		processedCues: make([]Cue, 0),
	}
}

// RenderFrame renders the next frame for all active cues
func (cm *CueMaster) RenderFrame(fm fixture.Manager, currentTime time.Time) {
	// Create a new metronome snapshot to align all effects
	snapshot := cm.metronome.GetSnapshot()

	// First, loop over all active effects and see if any of them need to end
	// activeEffects := make([]effect.Player, 0)
	// for _, effect := range cm.activeEffects {
	// 	if effect.IsActive() {
	// 		activeEffects =
	// 	}
	// }

	for _, cue := range cm.activeCues {
		cue.RenderFrame(fm, snapshot)
	}

	// TODO - we probably want logic like this later to ensure we are accurately maintaining the render loop.
	// It will be really important if the average frame render time is exceeding 25ms.
	//ended := time.Now()
	//duration := ended.Sub(snapshot.Instant
	//sleepTime := time.Duration(math.Max(1, float64(show.RefreshInterval-duration))))
	//time.Sleep(sleepTime)
}

// ProcessForever processes all of the cues
// func (cm *CueMaster) ProcessForever(ctx context.Context, wg *sync.WaitGroup) {
// 	wg.Add(1)

// 	// TODO - hardcoded for now
// 	// This is the rate when we are idly waiting for cues to process
// 	cueBackOff := time.Millisecond * 25
// 	defer wg.Done()

// 	//logger.Printf("ProcessCueList started at %v, name=%v", time.Now(), cl.Name)

// 	t := time.NewTimer(cueBackOff)
// 	defer t.Stop()

// 	for {
// 		select {
// 		case <-ctx.Done():
// 			//logger.Printf("ProcessCueList shutdown, name=%v", cl.Name)
// 			return //ctx.Err()
// 		case <-t.C:
// 			// pop a cue off the stack
// 			cue, pc := cm.pendingCues[0], cm.pendingCues[1:]
// 			cm.pendingCues = pc
// 			cm.activeCues = append(cm.activeCues, cue)
// 			wg.Add(1)

// 			// Process the Cue
// 			// cm.ProcessCue(cue, wg)

// 			// Post Processing Cleanup
// 			cm.processedCues = append(cm.processedCues, cue)
// 			t.Reset(0)
// 		}
// 	}
// }

// func (pw *progressWriter) Start() {
// 	// TeeReader calls pw.Write() each time a new response is received
// 	_, err := io.Copy(pw.file, io.TeeReader(pw.reader, pw))
// 	if err != nil {
// 		p.Send(progressErrMsg{err})
// 	}
// }
