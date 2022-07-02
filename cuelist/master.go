package cuelist

import (
	"context"
	"sync"
	"time"

	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/logger"
	"github.com/sirupsen/logrus"
	"k8s.io/utils/clock"
)

// MasterManager is an interface
type MasterManager interface {
	ProcessCueList(ctx context.Context, cl *CueList, wg *sync.WaitGroup)
	ProcessCue(c *Cue, wg *sync.WaitGroup)
	ProcessFrame(cf *Frame, wg *sync.WaitGroup)
	ProcessFrameAction(cfa *FrameAction, wg *sync.WaitGroup)
	EnQueueCue(c Cue, cl *CueList) *Cue
	AddIDsRecursively(c *Cue)
	GetDefaultCueList() *CueList
	ProcessForever(ctx context.Context, wg *sync.WaitGroup)
	GetFixtureManager() fixture.Manager
}

// Master is the parent of all Cue Lists and is a singleton.
type Master struct {
	CueLists       []CueList
	currentID      int64
	clock          clock.Clock
	idLock         sync.Mutex
	FixtureManager fixture.Manager
}

// Master singleton
var cueListMasterSingleton Master

// InitializeMaster initializes the Cue List Master
func InitializeMaster(cl clock.Clock, fm fixture.Manager) MasterManager {
	return &Master{
		currentID:      1,
		clock:          cl,
		CueLists:       []CueList{{Priority: 1, Name: "main"}},
		FixtureManager: fm,
	}
}

func (clm *Master) getNextIDForUse() int64 {
	clm.idLock.Lock()
	defer clm.idLock.Unlock()

	id := clm.currentID
	clm.currentID++
	return id
}

// GetDefaultCueList gives the first cuelist
func (clm *Master) GetDefaultCueList() *CueList {
	return &clm.CueLists[0]
}

// ProcessForever runs all the cuelists
func (clm *Master) ProcessForever(ctx context.Context, wg *sync.WaitGroup) {
	logger := logger.GetProjectLogger()
	logger.Info("Processing cue lists...")
	for i := range clm.CueLists {
		logger.Info("goty cue lists...")
		wg.Add(1)
		go clm.ProcessCueList(ctx, &clm.CueLists[i], wg)
		logger.Info("past goty cue lists...")
	}
}

// GetFixtureManager returns a poitner to the light state manager
func (clm *Master) GetFixtureManager() fixture.Manager {
	return clm.FixtureManager
}

// ProcessCueList processes cue lists
func (clm *Master) ProcessCueList(ctx context.Context, cl *CueList, wg *sync.WaitGroup) {
	// TODO - hardcoded for now
	cueBackOff := time.Millisecond * 25
	defer wg.Done()

	logger := logger.GetProjectLogger()
	logger.Printf("ProcessCueList started at %v, name=%v", time.Now(), cl.Name)

	t := time.NewTimer(cueBackOff)
	defer t.Stop()

	for {
		select {
		case <-ctx.Done():
			logger.Printf("ProcessCueList shutdown, name=%v", cl.Name)
			return //ctx.Err()
		case <-t.C:
			if nextCue := cl.deQueueNextCue(); nextCue != nil {
				cl.ActiveCue = nextCue
				nextCue.Status = statusActive
				nextCue.StartedAt = time.Now()
				wg.Add(1)
				clm.ProcessCue(nextCue, wg)
				// post processing cleanup
				nextCue.FinishedAt = time.Now()
				nextCue.Status = statusProcessed
				nextCue.RealDuration = nextCue.FinishedAt.Sub(nextCue.StartedAt)
				cl.ActiveCue = nil
				cl.ProcessedCues = append(cl.ProcessedCues, *nextCue)

				//update metrics
				// metrics.CueExecutionDrift.Set(nextCue.getDurationDrift().Seconds())
				// metrics.CueBacklogCount.WithLabelValues(cs.Name).Set(float64(len(cs.Cues)))
				// metrics.CueProcessedCount.WithLabelValues(cs.Name).Set(float64(len(cs.ProcessedCues)))
				t.Reset(0)
			} else {
				t.Reset(cueBackOff)
			}
		}
	}

}

// ProcessCue processes cue
func (clm *Master) ProcessCue(c *Cue, wg *sync.WaitGroup) {
	defer wg.Done()

	logger := logger.GetProjectLogger()
	logger.WithFields(logrus.Fields{"cue_id": c.ID, "cue_name": c.Name}).Info("ProcessCue")

	wg.Add(len(c.Frames))
	for _, eachFrame := range c.Frames {
		clm.ProcessFrame(&eachFrame, wg)
	}
}

// ProcessFrame processes the cueframe
func (clm *Master) ProcessFrame(cf *Frame, wg *sync.WaitGroup) {
	defer wg.Done()

	logger := logger.GetProjectLogger()
	logger.WithFields(logrus.Fields{"duration": cf.GetDuration(), "num_actions": len(cf.Actions)}).Info("ProcessFrame")

	wg.Add(len(cf.Actions))
	for x := range cf.Actions {
		go clm.ProcessFrameAction(&cf.Actions[x], wg)
	}
	// no blocking, so wait until all the child frames have theoretically finished
	clm.clock.Sleep(cf.GetDuration())
}

// ProcessFrameAction does the heavy lifting stuff
func (clm *Master) ProcessFrameAction(cfa *FrameAction, wg *sync.WaitGroup) {
	defer wg.Done()

	now := time.Now().UnixNano() / int64(time.Millisecond)

	logger := logger.GetProjectLogger()
	logger.WithFields(logrus.Fields{"duration": cfa.NewState.Duration, "now_ms": now, "fixture": cfa.FixtureName}).
		Infof("ProcessFrameAction (color=%v)", cfa.NewState.RGB.TermString())

	if l := clm.GetFixtureManager().GetByName(cfa.FixtureName); l != nil {
		go l.SetState(clm.FixtureManager, cfa.NewState)
	} else {
		logger.Errorf("Cannot find fixture by name: %s\n", cfa.FixtureName)
	}

	// goroutine doesn't block, so hold until the SetState has (hopefully) finished timing-wise
	// TODO: why are we doing this?
	clm.clock.Sleep(cfa.NewState.Duration)
}

// AddIDsRecursively populates the ID fields on a cue, its frames, and their actions
func (clm *Master) AddIDsRecursively(c *Cue) {
	c.Status = statusEnqueued
	if c.ID == 0 {
		c.ID = clm.getNextIDForUse()
	}
	for x := range c.Frames {
		eachFrame := &c.Frames[x]
		if eachFrame.ID == 0 {
			eachFrame.ID = clm.getNextIDForUse()
		}
		for y := range eachFrame.Actions {
			eachAction := &eachFrame.Actions[y]
			if eachAction.ID == 0 {
				eachAction.ID = clm.getNextIDForUse()
			}
			eachAction.Fixture = clm.GetFixtureManager().GetByName(eachAction.FixtureName)
		}
	}
}
