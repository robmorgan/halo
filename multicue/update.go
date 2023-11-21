package main

import (
	"math"
	"time"

	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

type cueProcessedMsg string

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "[":
			m.bpm--
		case "]":
			m.bpm++
		case "g":
			// pop the next cue off the stack
			var nextCue Cue
			nextCue, m.cueMaster.pendingCues = m.cueMaster.pendingCues[0], m.cueMaster.pendingCues[1:]
			m.cueMaster.activeCues = append(m.cueMaster.activeCues, nextCue)
		case "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	case cueProcessedMsg:
		//return m, waitForActivity(m.sub) // wait for next event
		// TODO - process the next cue
		return m, nil
	case tickMsg:
		//if m.progress.Percent() == 1.0 {
		//	return m, tea.Quit
		//}

		// Note that you can also use progress.Model.SetPercent to set the
		// percentage value explicitly, too.
		//cmd := m.progress.IncrPercent(0.25)
		//for i, _ := range m.cueMaster.activeCues {

		//p := m.activeProgress[i]
		//p.IncrPercent(0.25)
		//fmt.Println("foo")

		//}
		// TODO - get the next frame from all active cues
		// tell all active cues to render the next frame
		var effectVal int
		for _, cue := range m.cueMaster.activeCues {
			//t := msg.(time.Time)
			t := time.Time(msg)
			effectVal = cue.RenderFrame(t)

			// 	newModel, cmd := m.progress.Update(msg)
			// if newModel, ok := newModel.(progress.Model); ok {
			// 	m.progress = newModel
			// }
		}

		// prepare next dmx packet
		values := make([]byte, 512)

		// Turn on the Right PAR
		values[0] = byte(effectVal) // intensity
		values[1] = 255             // red

		// Turn on the Left PAR
		values[8] = byte(effectVal) // intensity
		values[9] = 255             // red

		values[32] = byte(effectVal) // intensity
		values[33] = 255             // red

		// send the packet
		if _, err := m.client.SendDmx(1, values); err != nil {
			tea.Printf("SendDmx: 1: %v", err)
		}

		m.progress += 0.1
		return m, tickCmd()

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd
	default:
		return m, nil
	}
	return m, nil
}

func (m model) processNextCue() tea.Cmd {
	// get the target cue
	//x, xs := xs[i], append(xs[:i], xs[i+1:]...)

	// pop a cue off the stack
	cue, pc := m.cueMaster.pendingCues[0], m.cueMaster.pendingCues[1:]
	m.cueMaster.pendingCues = pc

	// This is where you'd do i/o stuff to process the actual cue.
	// In our case we're just pausing for a moment to simulate the process.
	//d := time.Millisecond * time.Duration(rand.Intn(500))
	d := cue.FadeTime

	return tea.Tick(d, func(t time.Time) tea.Msg {
		return cueProcessedMsg("foo")
	})
}

func removeIndex(s []int, index int) []int {
	return append(s[:index], s[index+1:]...)
}

func clamp(t, min, max float64) float64 {
	min, max = math.Min(min, max), math.Max(min, max)
	return math.Max(math.Min(t, max), min)
}
