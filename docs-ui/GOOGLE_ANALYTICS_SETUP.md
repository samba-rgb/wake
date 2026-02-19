# Google Analytics Setup for Wake Documentation

This guide will help you complete the Google Analytics setup for tracking Wake documentation usage and installations.

## Step 1: Create Google Analytics Account

1. Go to [Google Analytics](https://analytics.google.com/)
2. Click "Start measuring" or "Get started"
3. Create an account (use your email/Google account)

## Step 2: Set Up Property

1. **Account Name**: "Wake Documentation"
2. **Property Name**: "Wake Docs"
3. **Reporting Time Zone**: Your timezone
4. **Currency**: Your preferred currency
5. **Industry Category**: "Technology" or "Software"
6. **Business Size**: "Small" (1-10 employees)

## Step 3: Configure Data Stream

1. Choose **"Web"** as platform
2. **Website URL**: `https://wakelog.in` (or `https://samba-rgb.github.io`)
3. **Stream Name**: "Wake Documentation Site"
4. Click **"Create Stream"**

## Step 4: Get Your Tracking ID

After creating the stream, you'll see a **Measurement ID** that looks like:
`G-XXXXXXXXXX` (for GA4)

## Step 5: Update Docusaurus Config

Replace `'G-XXXXXXXXXX'` in your `docusaurus.config.ts` file with your actual tracking ID.

## Step 6: Deploy Changes

```bash
# Commit and push the analytics changes
git add docs-ui/docusaurus.config.ts
git commit -m "Add Google Analytics tracking for installation metrics"
git push
```

## What You'll Track

Once deployed, you'll see:

### 📊 **Installation Metrics**
- Page views on installation guides
- Geographic distribution of users
- Popular installation methods (Homebrew vs source)
- Time spent on documentation pages

### 📈 **User Behavior**
- Most visited features (UI, web view, templates, etc.)
- User flow through documentation
- Popular search terms within docs
- Mobile vs desktop usage

### 🔍 **Key Pages to Monitor**
- `/docs/guides/installation` - Direct installation tracking
- `/docs/intro` - New user onboarding
- `/docs/features/web-view` - Feature adoption
- `/` - Landing page effectiveness

## Privacy & Compliance

✅ **GDPR Compliant**: IP anonymization enabled
✅ **No Personal Data**: Only aggregated statistics
✅ **Cookie Consent**: Handled automatically by Docusaurus

## Expected Data Timeline

- **Real-time data**: Available immediately
- **Full reports**: 24-48 hours after deployment
- **Historical trends**: After 1 week of data collection

This will give you much better installation tracking than GitHub's 14-day limit!