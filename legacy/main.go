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

	// initialize the global config
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

	// define a few convience fixture groups
	totemPARs := []string{"left_top_par", "left_middle_par", "left_bottom_par", "right_top_par", "right_middle_par", "right_bottom_par"}

	// init cue master
	logger.Info("Initializing cue list master...")
	master := cuelist.InitializeMaster(clock.RealClock{}, fm)
	//		master.SetCommands(c.Commands)

	/// build show
	cuelist := master.GetDefaultCueList()

	//
	// Track: Love Sensation
	//

	// Cue #0: ARM
	c := clearFixtures([]string{"left_middle_par", "right_middle_par"}, time.Second*1)
	master.EnQueueCue(*c, cuelist)

	// Cue #1
	// cycle middle pars
	stateA := fixture.State{Intensity: 200, RGB: utils.GetRGBFromString("#FFD700")}
	stateB := fixture.State{Intensity: 0, RGB: utils.GetRGBFromString("#FFD700")}
	//tickInterval := time.Millisecond * 60000 / 130
	//tickInterval := time.Millisecond * 30
	//tickInterval := utils.BPMToMilliseconds(130)
	c, err = cycleFixtureStates([]string{"left_middle_par", "right_middle_par"}, stateA, stateB, "16s", 35)
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}
	master.EnQueueCue(*c, cuelist)

	// Play all Totem PARs at once
	c, err = flashPARs(totemPARs, stateA, "1s", 10)
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}
	master.EnQueueCue(*c, cuelist)

	// Cue #2: Middle PARs off, Strobe top PARs
	c = getLoveSensationCue2()
	master.EnQueueCue(*c, cuelist)

	// clear the middle pars
	c = clearFixtures([]string{"left_middle_par", "right_middle_par"}, time.Millisecond*30)
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}
	master.EnQueueCue(*c, cuelist)

	// beam bars
	c, err = processCycleCommandBeams("10s")
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}
	master.EnQueueCue(*c, cuelist)

	// top pars
	c, err = processTopParsCommand("10s")
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}
	master.EnQueueCue(*c, cuelist)

	// led spot moving heads
	// shehds-led-wash-7x18w-rgbwa-uv
	c, err = processCycleCommandSpots("5s")
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}
	master.EnQueueCue(*c, cuelist)

	// led wash moving heads
	// shehds-led-wash-7x18w-rgbwa-uv
	c, err = processCycleCommandWashes("3s")
	if err != nil {
		logger.Fatalf("error processing cue. err='%v'", err)
	}
	master.EnQueueCue(*c, cuelist)

	// play audio
	go playAudio(LoveSensationAudioFile)

	// Hack: wait for the first beat (beat starts at 1s 21.28ms)
	time.Sleep(time.Millisecond * 1021)

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
