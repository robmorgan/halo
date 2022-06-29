package main

import (
	"fmt"
	"log"
	"time"

	"github.com/fogleman/ease"
	"github.com/nickysemenza/gola"
	"github.com/robmorgan/halo/cuelist"
	"github.com/robmorgan/halo/effect"
	"github.com/robmorgan/halo/engine"
	"github.com/robmorgan/halo/fixture"
	"github.com/robmorgan/halo/logger"
)

const (
	progressBarWidth  = 71
	progressFullChar  = "█"
	progressEmptyChar = "░"
	GlobalFPS         = 40
)

func main() {
	// initialize the logger
	logger := logger.GetProjectLogger()

	logger.Info("Connecting to OLA...")
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
	log.Println("Starting render loop")

	// lets make a color pulsing effect
	fx1 := effect.NewEffect(ease.InQuart, 3.0, 0.2)
	fx2 := effect.NewEffect(ease.OutCubic, 3.0, 1)

	//tick := time.Tick(40 * time.Millisecond)
	gl := engine.New(40*time.Millisecond, func(delta float64) {
		//log.Println(fmt.Printf("tick: %.7f", delta))
		values := make([]byte, 512, 512)

		//values[119] = 255

		// Turn on the Right PAR
		//values[138] = 255

		//values[141] = byte(dVal)
		//values[141] = 255

		par1, err := config.PatchedFixtures.Root.GetFixture("right_middle_par")
		if err != nil {
			panic(fmt.Sprintf("could not get fixture: %s", err))
		}

		par2, err := config.PatchedFixtures.Root.GetFixture("left_middle_par")
		if err != nil {
			panic(fmt.Sprintf("could not get fixture: %s", err))
		}

		par3, err := config.PatchedFixtures.Root.GetFixture("left_uplight_par")
		if err != nil {
			panic(fmt.Sprintf("could not get fixture: %s", err))
		}

		par4, err := config.PatchedFixtures.Root.GetFixture("right_uplight_par")

		log.Println(fmt.Printf("PAR4 Fix Addr: %p\n", par4))
		if err != nil {
			panic(fmt.Sprintf("could not get fixture: %s", err))
		}

		// set some nice colors
		par1.SetColorFromHex("#0000FF")
		par2.SetColorFromHex("#0000FF")
		par3.SetColorFromHex("#FF0000")
		par4.SetColorFromHex("#FF0000")

		// update front pars
		parIntensity := par1.GetIntensity()

		newIntensityVal := fx1.Update(delta, float64(parIntensity))
		log.Println(fmt.Sprintf("PAR oldVal=%.7f newVal=%.7f", parIntensity, newIntensityVal))
		par1.SetIntensity(newIntensityVal)
		par2.SetIntensity(newIntensityVal)
		if err != nil {
			panic(fmt.Sprintf("could not set intensity: %s", err))
		}

		// update uplighting pars
		parIntensity2 := par3.GetIntensity()

		newIntensityVal2 := fx2.Update(delta, float64(parIntensity2))
		log.Println(fmt.Sprintf("PAR3 oldVal=%.7f newVal=%.7f", parIntensity2, newIntensityVal2))
		par3.SetIntensity(newIntensityVal2)
		par4.SetIntensity(newIntensityVal2)
		if err != nil {
			panic(fmt.Sprintf("could not set intensity: %s", err))
		}

		// check all fixtures that need to update and render them
		for idx, f := range config.PatchedFixtures.Root.Fixtures {
			log.Println(fmt.Printf("Fix Addr: %p\n", f))

			if f.NeedsUpdate() {
				fmt.Printf("Fixture (%s) needs an update: %v\n", idx, f)

				// prepare DMX packet
				//sendColor, _ := fixture.GetColor()
				//values[fixture.Address+1] = byte(uint8(sendColor * 255))
				//values[fixture.Address] = 255 // let intensity be full
				//fixture.Bytes()

				// Loop over all of the channels
				for _, ch := range f.Channels {
					idx := f.Address + ch.Address - 1
					switch ch.Type {
					case fixture.TypeIntensity:
						//values[ch.Address] = 255
						values[idx] = byte(uint8(f.GetIntensity() * 255))
					case fixture.TypeColorRed:
						values[idx] = byte(uint8(f.Color.R * 255))
					case fixture.TypeColorGreen:
						values[idx] = byte(uint8(f.Color.G * 255))
					case fixture.TypeColorBlue:
						values[idx] = byte(uint8(f.Color.B * 255))
					}
				}

				// we've updated
				f.HasUpdated()
			}
		}

		if status, err := client.SendDmx(1, values); err != nil {
			log.Printf("Error Sending Dmx: %v", status)
		} //else {
		// We are okay
		//log.Printf("Error Sending Dmx: %v", status)
		//}

		// // get DMX on universe 1
		// if x, err := client.GetDmx(1); err != nil {
		// 	log.Printf("GetDmx: 1: %v", err)
		// } else {
		// 	log.Printf("GetDmx: 1: %v", x.Data)
		// }

		// TODO - we are currently sleeping because olad complains that there is "No buffer space available".
		// Somehow we'll need to make sure our updates are "real-time" enough, but don't overwhelm the process.
		//time.Sleep(1000 * time.Millisecond)
		time.Sleep(40 * time.Millisecond)
	})

	gl.Start()

	// Stop Game Loop:
	// gl.Stop()

	// get DMX on universe 1
	if x, err := client.GetDmx(1); err != nil {
		log.Printf("GetDmx: 1: %v", err)
	} else {
		log.Printf("GetDmx: 1: %v", x.Data)
	}

	// PAR 115
	// PAR 139
	// Don't stop main goroutine
	log.Printf("Going into terminal loop")
	for {
	}
}
