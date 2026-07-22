package main

import "terlan.dev/aotbench/internal/mathvalue"

func main() {
	if mathvalue.Value()+34 != 41 {
		panic("unexpected result")
	}
}
