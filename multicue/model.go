package main

import (
	"fmt"
	"time"

	"github.com/charmbracelet/bubbles/progress"
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/nickysemenza/gola"

	"github.com/robmorgan/halo/config"
	"github.com/robmorgan/halo/fixture"
)

type model struct {
	sub            chan struct{} // where we'll receive activity notifications
	bpm            int
	spinner        spinner.Model
	activeProgress []progress.Model // we reuse a pool of progress bars for active cues
	progress       float64
	cueMaster      CueMaster
	fixtureManager fixture.Manager
	quitting       bool
	client         *gola.Client
	config         config.HaloConfig
}

func newModel(client *gola.Client) model {
	s := spinner.New()
	s.Style = lipgloss.NewStyle().Foreground(lipgloss.Color("63"))

	// prepare a pool of progress bars
	pp := make([]progress.Model, 0)
	for i := 0; i < MaxActiveCues; i++ {
		p := progress.New(
			progress.WithDefaultGradient(),
			progress.WithWidth(40),
			progress.WithoutPercentage(),
		)

		pp = append(pp, p)
	}

	// Init Halo config
	config, err := config.NewHaloConfig()
	if err != nil {
		panic("error creating config")
	}

	// Init the CueMaster
	cm := CueMaster{}

	// Init the Fixture Manager
	fm, err := fixture.NewManager(config)
	if err != nil {
		panic(fmt.Sprintf("cannot initialize the fixture manager. err='%v'", err))
	}

	// Enqueue Cues
	cues := getCues()
	cm.pendingCues = cues

	return model{
		bpm:            130,
		client:         client,
		config:         config,
		cueMaster:      cm,
		fixtureManager: fm,
		spinner:        s,
		activeProgress: pp,
	}
}

func (m model) Init() tea.Cmd {
	return tea.Batch(tickCmd(), m.spinner.Tick)
}

type tickMsg time.Time

func tickCmd() tea.Cmd {
	return tea.Tick(time.Millisecond*25, func(t time.Time) tea.Msg {
		return tickMsg(t)
	})
}
