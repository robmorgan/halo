package main

import "github.com/charmbracelet/bubbles/progress"

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
	index          int
	cues           []Cue
	activeCues     []Cue
	activeProgress []progress.Model // we reuse a pool of progress bars for active cues
	processedCues  []Cue
	cuesProcessed  int // how many cues we've processed

	// actions
	// effects
}
