package main

import (
	"math/rand"
	"time"

	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

type cueProcessedMsg string

type enQueueCueMsg struct {
	cue Cue
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "[":
			m.bpm--
		case "]":
			m.bpm++
		case "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	case enQueueCueMsg:
		// we've recieved a new cue to process
		return m, processCue(msg.cue)
	case cueProcessedMsg:
		m.cuesProcessed++ // record external activity
		//return m, waitForActivity(m.sub) // wait for next event
		return m, nil
	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd
	default:
		return m, nil
	}
	return m, nil
}

func processCue(c Cue) tea.Cmd {
	// This is where you'd do i/o stuff to process the actual cue.
	// In our case we're just pausing for a moment to simulate the process.
	d := time.Millisecond * time.Duration(rand.Intn(500))
	return tea.Tick(d, func(t time.Time) tea.Msg {
		return cueProcessedMsg("foo")
	})
}
