package main

import (
	"context"
	"os"
	"os/signal"
	"sync"
	"time"

	"github.com/nickysemenza/gola"
	"github.com/robmorgan/halo/config"
	"github.com/robmorgan/halo/cuelist"
	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/logger"
	"github.com/robmorgan/halo/utils"
	"k8s.io/utils/clock"
)

const (
	progressBarWidth  = 71
	progressFullChar  = "█"
	progressEmptyChar = "░"
	GlobalFPS         = 40
)

func main() {
	// We don't process any CLI flags or config for now, so just run the app with a context.
	// TODO - add config to the context
	ctx := context.Background()
	Run(ctx)
}

// Run starts the console
func Run(ctx context.Context) {
	ctx, cancel := context.WithCancel(ctx)

	// initialize the logger
	logger := logger.GetProjectLogger()

	wg := sync.WaitGroup{}

	// initiailze the global config
	logger.Info("Initializing config...")
	config, err := config.NewHaloConfig()
	if err != nil {
		panic("error creating config")
	}

	// initialize the fixtures
	logger.Info("Initializing fixture manager...")
	fm, err := fixture.NewManager(config)
	if err != nil {
		logger.Fatalf("error initializing fixture manager. err='%v'", err)
	}

	// init cue master
	logger.Info("Initializing cue list master...")
	master := cuelist.InitializeMaster(clock.RealClock{}, fm)
	//		master.SetCommands(c.Commands)

	// build show
	cuelist := master.GetDefaultCueList()
	c, err := processCycleCommand("1s")
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}

	master.EnQueueCue(*c, cuelist)

	// process cues forever
	logger.Info("Processing cues forever...")
	master.ProcessForever(ctx, &wg)

	// configure OLA for DMX output
	logger.Info("Connecting to OLA...")
	olaTick := 40 * time.Millisecond
	client, err := gola.New("localhost:9010")
	if err != nil {
		logger.Errorf("could not connect to OLA: %v", err)
	} else {
		wg.Add(1)
		go fixture.SendDMXWorker(ctx, client, olaTick, fm, &wg)
	}
	defer client.Close()

	// handle CTRL+C interrupt
	quit := make(chan os.Signal)
	signal.Notify(quit, os.Interrupt)

	<-quit
	logger.Println("shutting down halo")
	cancel()
	wg.Wait()
}

// e.g. cycle(c1+c2+c3+c4+c5+c6:500ms)
func processCycleCommand(timeStr string) (*cuelist.Cue, error) {
	cue := cuelist.Cue{}

	fixtureList := []string{"left_middle_par", "right_middle_par"}
	duration, err := time.ParseDuration(timeStr)
	if err != nil {
		return nil, err
	}
	for x := range fixtureList {
		frame := cuelist.Frame{}
		for y := 0; y < len(fixtureList); y++ {
			action := cuelist.FrameAction{}
			action.FixtureName = fixtureList[y]

			action.NewState = fixture.TargetState{
				State:    fixture.State{RGB: utils.GetRGBFromString("#0000FF")},
				Duration: duration,
			}
			if x == y {
				action.NewState = fixture.TargetState{
					State:    fixture.State{RGB: utils.GetRGBFromString("#FF0000")},
					Duration: duration,
				}
			}

			frame.Actions = append(frame.Actions, action)
		}
		cue.Frames = append(cue.Frames, frame)
	}

	return &cue, nil
}
