# Mu Protocol Whitepaper

## Introduction

MuProtocol is a distributed, decentralized cloud provider marketplace. Mu allows infrastructure providers to offer their servers for sale in a standardized and secure way. Apps designed for Mu can work with any Mu-enabled provider, which removes the vendor lock from most today's cloud providers.

By creating a standardized way to provide and consume cloud technology, Mu allows clients and providers to create a healthier and more competitive ecosystem, where apps can be easily developed and tested locally, and deployed with the same ease to the cloud. 

Mu makes developers' life considerably easier by providing a single, open standards-based and simple way to develop, test and deploy Serverless applications. Developers need not to worry anymore where they will be deploying their application, all they have to do is to write it with the Mu toolkit and test it locally, where it will work **exactly the same way** as it would in the cloud. After the application is ready to be deployed, they simply choose a provider and a region from the Mu Marketplace and deploy it. They will then pay per request, CPU and database use, just like they would with traditional cloud providers.

Mu also simplifies the cloud providers' life. Mu is not just a standardized way to write and deploy applications, it is also a standardized way to **provide cloud infrastructure**. This way, providers need not to worry about which cloud to be compatible with, which services to provide, etc, instead, they simply run the Mu executor, Mu will then turn their nodes into Mu nodes that can already offer Mu hosting services to the world. The Mu executor is self-contained and has no external dependencies, no need to worry about Kubernetes or OpenShift anymore.

The Mu runtime is based in the upcoming [WASI](https://wasi.dev) standard, an open, high performance system interface for WebAssembly.

Mu consists in frou main components:
* The Mu smart contract
* The Mu Marketplace
* The Mu Executor
* The Mu Toolkit

## The Mu smart contract

The smart contract is the heart of the Mu ecosystem, it implements the logic required for the Marketplace and for running tasks in the Executor. 

The smart contract allows providers to register as such and then create regions and add nodes to each region. This way providers can be listed in the Marketplace and offer their cloud services to third parties. Providers must make a deposit in order to be listed, to guarantee only serious providers are part of the Marketplace.

The smart contract also allows clients to post their code to providers for execution. Clients have to make a prepayment to pay for the usage. Then they add all the job-related information in the transaction, such as environment variables, endpoints, etc. This information is encrypted using the provider's public key, so that only said provider can access this information, as it may contain secrets and/or other sensitive information.

After a job is deployed to the network, provider nodes will periodically report usage to the smart contract, allowing it to charge the client for said usage and transfer it to the provider's account.

All payments in the network are done using the network's native token, MU.

## The Mu Marketplace

The Mu Marketplace is where providers and clients interact with each other and with the network.

## The Mu Executor

## The Mu Toolkit