# GitHub Pages Deployment Guide

This guide will help you deploy the Wake documentation to GitHub Pages.

## Prerequisites

- GitHub repository with the documentation in the `docs-ui/` folder
- GitHub Actions enabled for your repository
- Node.js 18+ installed locally (for testing)

## Deployment Setup

### 1. Enable GitHub Pages

1. Go to your repository on GitHub
2. Navigate to **Settings** → **Pages**
3. Under **Source**, select **GitHub Actions**
4. The deployment workflow is already configured in `.github/workflows/deploy-docs.yml`

### 2. Repository Configuration

The repository is already configured with the correct settings in `docusaurus.config.ts`:

```typescript
url: 'https://samba-rgb.github.io',
baseUrl: '/wake/',
organizationName: 'samba-rgb',
projectName: 'wake',
```

### 3. Automatic Deployment

The documentation will automatically deploy when:
- You push changes to the `main` branch that affect files in `docs-ui/`
- The GitHub Action will build and deploy to GitHub Pages
- Your site will be available at: https://samba-rgb.github.io/wake/

## Manual Deployment (Alternative)

If you prefer to deploy manually using Docusaurus's built-in deployment:

```bash
cd docs-ui

# Set environment variables
export GIT_USER=samba-rgb
export DEPLOYMENT_BRANCH=gh-pages

# Deploy
npm run deploy
```

## Testing Locally

Before deploying, test your build locally:

```bash
cd docs-ui

# Build the site
npm run build

# Serve the built site
npm run serve
```

The site will be available at http://localhost:3000/wake/

## Workflow Details

The GitHub Actions workflow (`.github/workflows/deploy-docs.yml`) will:

1. **Trigger** on pushes to `main` branch that modify `docs-ui/` files
2. **Build** the Docusaurus site using Node.js 18
3. **Deploy** to GitHub Pages automatically
4. **Cache** dependencies for faster builds

## Custom Domain (Optional)

To use a custom domain:

1. Add your domain to `static/CNAME` file
2. Update the `url` in `docusaurus.config.ts`
3. Configure your domain's DNS to point to GitHub Pages

## Troubleshooting

### Build Failures
- Check the Actions tab in GitHub for build logs
- Ensure all dependencies are properly listed in `package.json`
- Verify there are no broken links or missing files

### Search Not Working
- Search functionality requires the site to be built and deployed
- Search index is generated during the build process
- Local development may show "Search not available" message

### Routing Issues
- Ensure `baseUrl` matches your repository name
- Check that all internal links use relative paths
- Verify the `trailingSlash` configuration if needed

## Site Features

Once deployed, your site will include:

✅ **Responsive Documentation** - Works on all devices
✅ **Search Functionality** - Full-text search across all docs
✅ **Dark/Light Mode** - Automatic theme switching
✅ **Interactive Navigation** - Collapsible sidebars and breadcrumbs
✅ **Fast Loading** - Optimized static site generation
✅ **SEO Optimized** - Meta tags and structured data

## Next Steps

1. **Push to GitHub** - Commit all changes and push to the `main` branch
2. **Watch Deployment** - Monitor the GitHub Actions tab for deployment progress
3. **Verify Site** - Check https://samba-rgb.github.io/wake/ once deployment completes
4. **Update Links** - Update any external references to point to the new documentation site

Your Wake documentation site will be live and automatically updated with every push to main!