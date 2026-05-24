const supported =
  process.versions.node
    .split('.')
    .map(Number)
    .slice(0, 2)

const [major, minor] = supported

if (
  !(
    (major === 20 && minor >= 19) ||
    (major === 22 && minor >= 13) ||
    major >= 24
  )
) {
  console.error(
    `frontend requires Node ^20.19.0, ^22.13.0, or >=24. Current: ${process.version}.`,
  )
  console.error('Run `asdf install nodejs 24 && asdf local nodejs 24`, `nvm use`, or `mise use node@24`.')
  process.exit(1)
}
