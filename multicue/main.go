package main

import (
	"fmt"
	"math/rand"
	"os"
	"time"

	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

func main() {
	rand.Seed(time.Now().UTC().UnixNano())

	p := tea.NewProgram(model{
		sub:     make(chan struct{}),
		spinner: spinner.New(),
	})

	if p.Start() != nil {
		fmt.Println("could not start program")
		os.Exit(1)
	}
}
