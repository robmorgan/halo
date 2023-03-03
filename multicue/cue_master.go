package main

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
	pendingCues   []Cue
	activeCues    []Cue
	processedCues []Cue

	// actions
	// effects
}

func (cm *CueMaster) ProcessCueList() {

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
