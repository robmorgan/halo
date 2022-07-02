package main

import (
	"sync"

	"github.com/robmorgan/halo/config"
	"github.com/robmorgan/halo/cuelist"
	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/logger"
	"k8s.io/utils/clock"
)

const (
	progressBarWidth  = 71
	progressFullChar  = "█"
	progressEmptyChar = "░"
	GlobalFPS         = 40
)

func main() {
	// We don't process any CLI flags or config for now, so just run the app.
	Run()
}

// Run starts the console
func Run() {
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

	// process cues forever
	logger.Info("Processing cues forever...")
	master.ProcessForever(&wg)
}
