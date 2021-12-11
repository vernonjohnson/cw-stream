# CW Stream

This contract enables the creation of cw20 token streams, allowing a cw20 payment to be vested continuously over time. The contract must be instantiated with a cw20 token address, after which any number of payment streams can be created from a single contract instance.

# Instantiation


# Creating a Stream
A stream can be created using the cw20 Send / Receive flow. A stream can be created by calling CW20ExecuteMsg::Send with the cw20_stream::CreateStream messsage as a callback message. 

# Withdrawing payments
Stream payments can be claimed by the recipient using the withdraw method. This can be called as long as some time has passed between the previous call. 


Compiling Contract

cargo compile

cargo test


