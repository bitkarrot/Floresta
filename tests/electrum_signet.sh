#!/bin/bash

# Starts a signet node and connects to it with Electrum, check if we can sync the wallet.
# This script syncs our node from genesis on signet. It might take some time and use
# some bandwidth and CPU. This is meant to mimic a simple setup, where you give a wallet's
# xpub and connect to floresta using Electrum.

# Note: To run this script, you need to set a wallet's xpub and seed, defining WALLET_XPUB
# and WALLET_SEED

ELECTRUM_NAME=Electrum
ELECTRUM_VERSION=4.4.4
ELECTRUM_URL=https://download.electrum.org/$ELECTRUM_VERSION/$ELECTRUM_NAME-$ELECTRUM_VERSION.tar.gz
ELECTRUM_TARBALL=$ELECTRUM_NAME-$ELECTRUM_VERSION.tar.gz
ELECTRUM_DIR=$PWD/$ELECTRUM_NAME-$ELECTRUM_VERSION/
FLORESTA_CLI=$PWD/target/debug/floresta-cli

if [ -z "$WALLET_SEED" ]
then
    echo "No wallet seed provided, try running"
    echo "$ export WALLET_SEED=\"your seed here\""
    exit
fi

# TODO: Maybe get the xpub from Electrum
if [ -z $WALLET_XPUB ]
then
    echo "No wallet xpub provided, try running"
    echo "$ export WALLET_XPUB=\"your xpub here\""
    exit
fi

get_electrum() {
    if [ ! -d $ELECTRUM_DIR ]; then
        wget $ELECTRUM_URL
        tar -xf $ELECTRUM_TARBALL
        rm $ELECTRUM_TARBALL
        $ELECTRUM_DIR/contrib/make_libsecp256k1.sh
    fi
}
exit_script() {
    echo "Stopping node and electrum"
    kill $ELECTRUM_PID
    kill $FLORESTA_PID
    rm -rf $FLORESTA_DATA_DIR
    rm -rf $ELECTRUM_USER_DIR
    exit
}

wait_sync() {
    echo "Waiting for the node to sync up"
    while true; do
        IBD="$($FLORESTA_CLI -n signet getblockchaininfo | jq -r ".ibd")"
        if [ $IBD == "false" ]; then
            break
        fi
        sleep 10
    done
    echo "Synced!"
}
# Download electrum, if not already downloaded
get_electrum

FLORESTA_DATA_DIR=$(mktemp -d)

# Start signet node
echo "Starting signet node..."
$PWD/target/release/floresta -n signet run --wallet-xpub $WALLET_XPUB --data-dir $FLORESTA_DATA_DIR &> /dev/null &
FLORESTA_PID=$!

# Start electrum
echo "Starting electrum..."
ELECTRUM_USER_DIR=$(mktemp -d)
ELECTRUM_SIGNET_SERVER_HOST="localhost"
ELECTRUM_SIGNET_SERVER="localhost:50001:t"
ELECTRUM_CMD="$ELECTRUM_DIR/run_electrum --signet -D $ELECTRUM_USER_DIR"

# Start electrum using the "one server option", so we have to sync through our server
$ELECTRUM_CMD daemon -s $ELECTRUM_SIGNET_SERVER -1 &> /dev/null &
ELECTRUM_PID=$!

# Wait for electrum to start
sleep 5

# Create wallet
echo "Creating wallet..."
$ELECTRUM_CMD restore "$WALLET_SEED" &> /dev/null
$ELECTRUM_CMD load_wallet -w $ELECTRUM_USER_DIR/signet/wallets/default_wallet &> /dev/null

# Check if we are connected to our server
echo "Checking if we are connected to our server..."
RES=$($ELECTRUM_CMD getinfo | jq ".connected")

if [ $RES != "true" ]; then
    echo "Not connected to our server"
    exit_script
fi

wait_sync

# Check our balance
echo "Checking balance..."
RES=$($ELECTRUM_CMD getbalance | jq ".confirmed")
if [ "$RES" == "0.00000000" ]; then
    echo "Balance is 0"
    exit_script
fi

echo "Sending one transaction"
ADDR=$($ELECTRUM_CMD createnewaddress)
TX=$($ELECTRUM_CMD --signet payto $ADDR 0.0001)

# Send if through mempool.space, because if we have an invalid wallet state, the tx
# will likely spend invalid UTXOs
TX_ID=$(curl -X POST -d $TX https://mempool.space/signet/api/tx)
echo $TX_ID

exit_script
