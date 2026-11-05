# 1. Introduction: what a VNA measures

## A wave out, a wave back

A VNA sends a sine wave of known frequency into a port and measures two things: how much
of it comes back (reflection) and how much comes out at the other port (transmission).
It does so not only in magnitude but also in phase, hence *vector*. It then moves the
frequency up one step and repeats this, hundreds or thousands of times: a
*sweep*.

The LiteVNA has two ports. Port 1 transmits and measures what comes back; port 2 only
listens. With that it measures two of the four S-parameters of a two-port:

- **S11**: the reflection at port 1. How well is the connected device matched to 50 Ω?
  This is the quantity for antennas, terminated cables and anything you measure at a single
  connector.
- **S21**: the transmission from port 1 to port 2. How much gets through, and with what
  delay? This is the quantity for filters, attenuators, amplifiers and cables used as a
  through connection.

S12 and S22 (the other direction) cannot be measured by the LiteVNA; in the export files
they are written as zero.

## The reflection coefficient Γ

Everything that is said about S11 is about the reflection coefficient Γ (gamma): the
complex number that says how large, and with what phase, the reflected wave is relative
to the outgoing one. |Γ| = 0 means no reflection, a perfect 50 Ω load;
|Γ| = 1 means everything comes back, an open or short-circuited end. Everything that is
interesting lies in between.

From Γ follow the quantities you use in practice:

| quantity | formula | meaning |
|---|---|---|
| S11 in dB | 20·log₁₀\|Γ\| | return loss; −10 dB = 10% of the power reflected, −20 dB = 1%, −30 dB = 0.1% |
| SWR | (1 + \|Γ\|)/(1 − \|Γ\|) | standing wave ratio; 1.0 is perfect, 2.0 (−9.5 dB) is the usual limit for transmitters, 3.0 is −6 dB |
| impedance Z | 50·(1 + Γ)/(1 − Γ) | the resistance R and reactance X that the port "sees": R + jX in ohms |
| phase | arg Γ | the angle; together with \|Γ\| it determines where you are on the Smith chart |

The relation between dB and SWR is not linear, and that takes its toll when reading values:
going from SWR 1.5 to 1.2 is going from 14 to 21 dB return loss, a factor of five less reflected
power. For tuning, dB is therefore often the finer measure; for the question "can my
transmitter be connected to this?" SWR is the custom.

## Transmission: S21

S21 in dB is the gain or attenuation from port 1 to port 2. A good cable sits close
to 0 dB and slowly drops away at higher frequencies, a 10 dB attenuator sits at
−10 dB, a filter shows its passband at almost 0 dB and its stopband at −40 dB or
deeper. The phase of S21 decreases linearly with frequency when only delay is
involved; its slope is the **group delay** τg = −dφ/dω: the transit time through the
component. For a cable it is flat, for a filter it rises at the band edges.

## The Smith chart

The Smith chart is the complex plane of Γ: the centre is Γ = 0 (50 Ω), the edge is
|Γ| = 1. The circles and arcs in it are lines of constant R and constant X, so that you
see at a glance whether something is too high or too low in impedance (right or left of the centre)
and whether it is inductive or capacitive (above or below the horizontal axis). A cable with
something at the end draws a spiral around that end point, because the phase rotates with
frequency; the longer the cable, the faster.

## Why calibrate

The device does not measure Γ directly but a version of it that has been distorted by its own bridge,
amplifiers and connectors. Without correction, the LiteVNA easily has
an error of |Γ| ≈ 0.1 in there, and above 1 GHz much more. Calibrating means: measuring three known
terminations (open, short, load) and computing from them, per frequency, the three error terms of
the device, after which every later measurement can be corrected. Chapter 4 covers
this in detail. The short advice: never measure without it, except to see whether there is
anything connected at all.

## How vna-r is put together

vna-r consists of three parts:

- **litevna**, the library: the USB protocol, sweeps, calibration, time domain, derived
  quantities, Touchstone files. No user interface; everything that computes lives here,
  with tests.
- **vna-cli**, the command line: every function of the library as a command, for
  scripts, quick checks and for those who prefer typing.
- **vna-gui**, the window: live traces, calibration panel, markers, TDR, references,
  files.

Calibrations, measurements and settings are stored on the computer, not in the
device. The calibration shown on the LiteVNA's own screen plays no role for vna-r:
the program asks the device for raw measurements and applies the correction itself.
