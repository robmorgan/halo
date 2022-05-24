package main

import (
	"log"
	"time"

	"github.com/nickysemenza/gola"
	"github.com/robmorgan/halo/cuelist"
	"github.com/robmorgan/halo/fixture"
)

const (
	progressBarWidth  = 71
	progressFullChar  = "█"
	progressEmptyChar = "░"
	GlobalFPS         = 40
)

func main() {
	client, err := gola.New("localhost:9010")
	if err != nil {
		panic("could not create client")
	}
	defer client.Close()

	// get DMX on universe 1
	if x, err := client.GetDmx(1); err != nil {
		log.Printf("GetDmx: 1: %v", err)
	} else {
		log.Printf("GetDmx: 1: %v", x.Data)
	}

	// initiailze the global config
	config, err := NewHaloConfig()
	if err != nil {
		panic("error creating config")
	}

	// initialize all fixtures
	config.Fixtures = map[string]interface{}{
		"center_left_par": &fixture.Fixture{
			Id:      1,
			Address: 138,
			Mode:    8,
			Channels: map[int]fixture.FixtureChannel{
				1: {
					Type:       "Intensity",
					Address:    1,
					Resolution: 1,
				},
			},
		},
	}

	// Prepare some sequences
	//move := NewSequence()
	// there should be a SetPosition
	// then maybe movement is actually handled by an effect.
	// effects could have oscillators and everything else is a static look.
	// there could also be an optional bounding box for a movement effect.
	// The CueList engine handles fades between looks

	// Create a new cue
	cuelist.NewCue("drop", func() {
		// TODO - turn on some fixtures

		// TODO - position them

		// TODO - add individual effects like oscillating

		// TODO - add group effects (like random PAR sparkle)
	})

	// start render loop
	// TODO - move to lighting engine
	tick := time.Tick(40 * time.Millisecond)

	for {
		select {
		case <-tick:
			values := make([]byte, 512, 512)
			//t := ease.InQuart(float64(i) / 255)
			//	dVal := int(t * 255)

			//values[119] = 255

			// Turn on the Right PAR
			values[138] = 255
			//values[141] = byte(dVal)
			values[141] = 255

			if status, err := client.SendDmx(1, values); err != nil {
				log.Printf("SendDmx: 1: %v", err)
			} else {
				log.Printf("SendDmx: 1: %v", status)
			}

			// // get DMX on universe 1
			// if x, err := client.GetDmx(1); err != nil {
			// 	log.Printf("GetDmx: 1: %v", err)
			// } else {
			// 	log.Printf("GetDmx: 1: %v", x.Data)
			// }
		}
	}

	// get DMX on universe 1
	if x, err := client.GetDmx(1); err != nil {
		log.Printf("GetDmx: 1: %v", err)
	} else {
		log.Printf("GetDmx: 1: %v", x.Data)
	}

	// PAR 115
	// PAR 139
}
