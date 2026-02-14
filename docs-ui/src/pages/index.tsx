import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import Heading from '@theme/Heading';
import useBaseUrl from '@docusaurus/useBaseUrl';

import styles from './index.module.css';

function HomepageHero() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero', styles.heroBanner)}>
      <div className="container">
        <div className={styles.heroContent}>
          <div className={styles.heroText}>
            <img src={useBaseUrl('/img/logo.png')} alt="Wake Logo" className={styles.heroLogo} />
            <Heading as="h1" className={styles.heroTitle}>
              <span className={styles.wakeName}>Wake</span>
            </Heading>
            <p className={styles.heroSubtitle}>
              Advanced Kubernetes Log Analysis Platform
            </p>
            <div className={styles.heroDescription}>
              <p>
                Multi-pod log analysis with <strong>real-time filtering</strong>, <strong>interactive TUI</strong>, 
                <strong> web dashboards</strong>, and <strong>automated diagnostics</strong>. Features advanced 
                pattern matching, template-based operations (JFR, heap dumps), script execution, and intelligent 
                search across massive Kubernetes clusters.
              </p>
            </div>
            <div className={styles.heroButtons}>
              <Link
                className="button button--primary button--lg"
                to="/docs/intro">
                Get Started
              </Link>
              <Link
                className="button button--secondary button--lg"
                to="/docs/guides/installation">
                Install
              </Link>
            </div>
          </div>
          <div className={styles.heroDemo}>
            <div className={styles.terminalWindow}>
              <div className={styles.terminalHeader}>
                <div className={styles.terminalButtons}>
                  <span className={styles.terminalButton}></span>
                  <span className={styles.terminalButton}></span>
                  <span className={styles.terminalButton}></span>
                </div>
                <span className={styles.terminalTitle}>Terminal</span>
              </div>
              <div className={styles.terminalBody}>
                <div className={styles.terminalLine}>
                  <span className={styles.prompt}>$</span>
                  <span className={styles.command}>brew install samba-rgb/wake/wake</span>
                </div>
                <div className={styles.terminalLine}>
                  <span className={styles.prompt}>$</span>
                  <span className={styles.command}>wake -n production --ui</span>
                </div>
                <div className={styles.terminalLine}>
                  <span className={styles.output}>✓ Monitoring logs from 5 pods...</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </header>
  );
}

function QuickStart() {
  return (
    <section className={styles.quickStart}>
      <div className="container">
        <div className={styles.sectionHeader}>
          <Heading as="h2" className={styles.sectionTitle}>
            Get Started in 2 Steps
          </Heading>
          <p className={styles.sectionSubtitle}>
            Start monitoring your Kubernetes logs in under a minute
          </p>
        </div>
        <div className={styles.quickStartGrid}>
          <div className={styles.step}>
            <div className={styles.stepNumber}>1</div>
            <div className={styles.stepContent}>
              <h3>Install Wake</h3>
              <div className={styles.codeWrapper}>
                <code>brew install samba-rgb/wake/wake</code>
                <span className={styles.orText}>or</span>
                <code>cargo install --git https://github.com/samba-rgb/wake</code>
              </div>
              <p className={styles.stepNote}>Available via Homebrew or build from source</p>
            </div>
          </div>
          <div className={styles.step}>
            <div className={styles.stepNumber}>2</div>
            <div className={styles.stepContent}>
              <h3>Start Monitoring</h3>
              <div className={styles.codeWrapper}>
                <code>wake --ui</code>
                <span className={styles.orText}>or</span>
                <code>wake -n your-namespace --ui</code>
              </div>
              <p className={styles.stepNote}>Launch interactive mode to view logs in real-time</p>
            </div>
          </div>
        </div>
        <div className={styles.nextSteps}>
          <h3>What's Next?</h3>
          <div className={styles.nextStepsList}>
            <div className={styles.nextStep}>
              <span className={styles.nextStepIcon}>🔍</span>
              <span>Press <kbd>i</kbd> in UI mode to filter logs</span>
            </div>
            <div className={styles.nextStep}>
              <span className={styles.nextStepIcon}>🌐</span>
              <span>Try web view with <code>--web</code> flag</span>
            </div>
            <div className={styles.nextStep}>
              <span className={styles.nextStepIcon}>📊</span>
              <span>Monitor resources with built-in metrics</span>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={`${siteConfig.title} - Kubernetes Log Analysis`}
      description="Simple, powerful Kubernetes log analysis with real-time filtering, interactive UI, and web viewing.">
      <HomepageHero />
      <main>
        <QuickStart />
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
