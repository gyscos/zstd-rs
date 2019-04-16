#!/bin/bash
cd zstd
CURRENT=$(git describe --tags)
git fetch
TAG=$(git tag -l | grep '^v' | sort | tail -n 1)

if [ $CURRENT != $TAG ]
then
    git checkout $TAG
    cd ..
    git add zstd
    ./update_bindings.sh
    git add src/bindings*.rs

    # Note: You'll need a forked version of cargo-dump that supports metadata
    # For instance https://github.com/gyscos/cargo-dump
    METADATA="zstd.${TAG/v/}"
    cargo bump patch --metadata $METADATA > /dev/null
    git add Cargo.toml
    cd ..
    cargo bump patch --metadata $METADATA > /dev/null
    git add Cargo.toml
    cd ..
    V=$(cargo bump patch --metadata $METADATA)
    V=$(echo $V | cut -d' ' -f4 | cut -d'+' -f1)

    git add Cargo.toml

    git commit -m "Update zstd to $TAG"

    # Publish?
    read -p "Publish to crates.io? " -n 1 -r
    echo $REPLY
    if [[ $REPLY =~ ^[Yy]$ ]]
    then
        cd zstd-safe/zstd-sys
        cargo publish
        cd ..
        cargo publish
        cd ..
        cargo publish
        git tag $V
    else
        echo "Would have published $V"
    fi

else
    echo "Already using zstd $TAG"
fi

