package main

import (
	"fmt"
	"math/rand"
	"os"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/nickysemenza/gola"
)

var p *tea.Program

func main() {
	rand.Seed(time.Now().UTC().UnixNano())

	// Init gola client
	client, err := gola.New("localhost:9010")
	if err != nil {
		panic("could not create client")
	}
	defer client.Close()

	if err := tea.NewProgram(newModel(client)).Start(); err != nil {
		fmt.Println("Error running program:", err)
		os.Exit(1)
	}
}

// func main() {
//

// 	p := tea.NewProgram(model{
// 		sub:     make(chan struct{}),
// 		spinner: spinner.New(),
// 	})

// 	if p.Start() != nil {
// 		fmt.Println("could not start program")
// 		os.Exit(1)
// 	}
// }
