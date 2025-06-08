# Pinocchio Escrow Program

This program implements a simple escrow service on the Solana blockchain, allowing users to exchange tokens in a trustless manner.

## Business Logic

The escrow program facilitates the exchange of two different tokens between a maker and a taker. The maker creates an escrow, specifying the tokens to be exchanged and the desired amounts. The taker can then take the escrow, effectively swapping the tokens. A refund mechanism is also provided, allowing the maker to reclaim their tokens if the escrow is not taken.

## Built with Pinocchio

This program was built using the [Pinocchio](https://github.com/anza-xyz/pinocchio) library, a framework for developing Solana programs with a focus on security and efficiency.

## Tests with Mollusk

The program includes a comprehensive suite of tests built using the [mollusk](https://github.com/anza-xyz/mollusk) library, providing a a lightweight test harness for Solana programs. It provides a simple interface for testing Solana program executions in a minified Solana Virtual Machine (SVM) environment.

It does not create any semblance of a validator runtime, but instead provisions a program execution pipeline directly from lower-level SVM components.

## Setup

To build and run this program, you will need to have Rust (rustc 1.87.0) and Solana (solana-cli 2.1.22) installed.

### How to Run Tests

To run the tests, use the following command:

```sh
make test
```
