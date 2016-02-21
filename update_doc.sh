set -e

cargo doc
FROM=$(git rev-parse --short HEAD)
git checkout gh-pages
rm zstd -rf
rsync -a target/doc/ .
git add .
git commit -m "Update doc for ${FROM}"
# git push
git checkout master
git submodule update
