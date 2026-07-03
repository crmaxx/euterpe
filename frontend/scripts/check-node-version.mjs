const [major] = process.versions.node.split('.').map(Number)

if (major !== 24) {
  console.error(
    `frontend requires Node 24.x. Current: ${process.version}.`,
  )
  console.error('Run `nvm use`, or `mise install && mise exec -- npm --prefix frontend run lint`.')
  process.exit(1)
}
