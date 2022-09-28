package main

import (
	"fmt"
	"math/rand"
	"time"

	"github.com/charmbracelet/bubbles/progress"
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

// A message used to indicate that activity has occurred. In the real world (for
// example, chat) this would contain actual data.
//type responseMsg struct{}

type cueProcessedMsg struct{}

type enQueueCueMsg struct {
	cue Cue
}

// Simulate a process that sends events at an irregular interval in real time.
// In this case, we'll send events on the channel at a random interval between
// 100 to 1000 milliseconds. As a command, Bubble Tea will run this
// asynchronously.
func listenForActivity(sub chan struct{}) tea.Cmd {
	return func() tea.Msg {
		for {
			time.Sleep(time.Millisecond * time.Duration(rand.Int63n(5000)+500))
			sub <- struct{}{}
		}
	}
}

// A command that waits for the activity on a channel.
func waitForActivity(sub chan struct{}) tea.Cmd {
	return func() tea.Msg {
		return enQueueCueMsg(<-sub)
	}
}

func processCue(c Cue) tea.Cmd {

}

type model struct {
	sub           chan struct{} // where we'll receive activity notifications
	cuesProcessed int           // how many cues we've processed
	spinner       spinner.Model
	activeCues    []Cue
	quitting      bool

	// TODO - properties to implement
	// FixtureManager
	// CueMaster
	// MasterTempo
}

type Cue struct {
	name     string
	progress progress.Model
	// actions
	// effects
}

func (m model) Init() tea.Cmd {
	return tea.Batch(
		spinner.Tick,
		listenForActivity(m.sub), // generate activity
		waitForActivity(m.sub),   // wait for activity
	)
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	case enQueueCueMsg:
		// we've recieved a new cue to process
		return m, processCue(msg.cue)
	case cueProcessedMsg:
		m.cuesProcessed++                // record external activity
		return m, waitForActivity(m.sub) // wait for next event
	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd
	default:
		return m, nil
	}
}

// TODO - render a progress bar for each cue.
// TODO - show active cue count
func (m model) View() string {
	s := fmt.Sprintf("\n %s Cues processed: %d\n\n Press ctrl+c to exit\n", m.spinner.View(), m.cuesProcessed)
	if m.quitting {
		s += "\n"
	}
	return s
}
