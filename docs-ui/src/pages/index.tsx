import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import Heading from '@theme/Heading';

import styles from './index.module.css';

const backgroundLogs = [
  '00:43:58.917 INFO  api-gateway       request completed latency=42ms',
  '00:43:58.411 WARN  payment-service   retrying provider call',
  '00:43:57.909 ERROR inventory-worker  database connection failed',
  '00:43:57.403 INFO  auth-service      token verified pod=auth-service-7c9d5f',
  '00:43:56.900 INFO  log-collector     streaming 18 pods from production',
  '00:43:56.394 WARN  profile-api       slow query detected duration=811ms',
];

const workflow = [
  {
    title: 'Collect the window',
    text: 'Pull enough logs to preserve the incident timeline instead of chasing isolated lines.',
    code: 'wake -n production api-* --since 2h --web',
  },
  {
    title: 'Query the signal',
    text: 'Use OpenObserve fields like level, message, pod_name, namespace, and time.',
    code: "WHERE level = 'error'",
  },
  {
    title: 'Keep data clean',
    text: 'Reconnects avoid replaying the same since window, so new rows stay new.',
    code: 'fresh logs only after reconnect',
  },
];

function HomepageHero() {
  return (
    <header className={styles.hero}>
      <div className={styles.backgroundLogs} aria-hidden="true">
        {[...backgroundLogs, ...backgroundLogs, ...backgroundLogs].map((line, index) => (
          <span key={`${line}-${index}`}>{line}</span>
        ))}
      </div>
      <div className="container">
        <div className={styles.heroGrid}>
          <div className={styles.heroCopy}>
            <img className={styles.heroIcon} src="/img/logo.png" alt="Wake icon" />
            <Heading as="h1" className={styles.heroTitle}>
              Wake
            </Heading>
            <p className={styles.heroLead}>One command center for Kubernetes logs and runtime debugging.</p>
            <p className={styles.heroText}>
              Wake follows logs across pods, keeps the surrounding context, filters noisy streams, remembers useful commands, and helps run diagnostics like scripts, templates, dumps, and resource checks from one focused CLI.
            </p>
            <div className={styles.heroActions}>
              <Link className="button button--primary button--lg" to="/docs/guides/installation">
                Install Wake
              </Link>
              <Link className="button button--secondary button--lg" to="/docs/intro">
                Read Docs
              </Link>
            </div>
            <div className={styles.commandGroup} aria-label="Wake quick start commands">
              <div className={styles.commandBox}>
                <span>$</span>
                <code>brew install samba-rgb/wake/wake</code>
              </div>
              <div className={styles.commandBoxSecondary}>
                <span>$</span>
                <code>wake -n production api-* --since 2h --ui</code>
              </div>
            </div>
            <div className={styles.platformPills}>
              <span>Apple Silicon ready</span>
              <span>Intel Mac not ready yet</span>
            </div>
          </div>
          <div className={styles.heroPanel} aria-label="Wake log preview">
            <div className={styles.panelHeader}>
              <span>logs stream</span>
              <strong>production</strong>
            </div>
            <div className={styles.panelLine}>
              <span className={styles.info}>INFO</span>
              <code>api-gateway request completed latency=42ms</code>
            </div>
            <div className={styles.panelLine}>
              <span className={styles.warn}>WARN</span>
              <code>payment-service retrying provider call</code>
            </div>
            <div className={styles.panelLine}>
              <span className={styles.error}>ERROR</span>
              <code>inventory-worker database connection failed</code>
            </div>
            <div className={styles.queryHint}>
              <span>query later</span>
              <code>WHERE level = 'error'</code>
            </div>
          </div>
        </div>
      </div>
    </header>
  );
}

function Workflow() {
  return (
    <section className={styles.workflowSection}>
      <div className="container">
        <div className={styles.sectionHeader}>
          <Heading as="h2" className={styles.sectionTitle}>
            Pull once. Query with context.
          </Heading>
          <p className={styles.sectionText}>
            Capture the full log window first, then narrow the investigation without losing the events around it.
          </p>
        </div>
        <div className={styles.workflowGrid}>
          {workflow.map((item, index) => (
            <article className={styles.workflowCard} key={item.title}>
              <span className={styles.stepNumber}>{index + 1}</span>
              <h3>{item.title}</h3>
              <p>{item.text}</p>
              <code>{item.code}</code>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

function PlatformNotice() {
  return (
    <section className={styles.noticeSection}>
      <div className="container">
        <div className={styles.notice}>
          <div>
            <Heading as="h2" className={styles.noticeTitle}>
              Platform Status
            </Heading>
            <p>
              Wake currently supports Apple Silicon Macs. Intel Mac support is still in progress and should not be treated as usable yet.
            </p>
          </div>
          <Link className={styles.textLink} to="/docs/guides/installation">
            Installation guide
          </Link>
        </div>
      </div>
    </section>
  );
}

function QuickStart() {
  return (
    <section className={styles.quickStart}>
      <div className="container">
        <div className={styles.quickStartGrid}>
          <div>
            <Heading as="h2" className={styles.sectionTitle}>
              Start with two commands.
            </Heading>
            <p className={styles.sectionText}>
              Install Wake on Apple Silicon, then point it at the namespace you care about.
            </p>
          </div>
          <div className={styles.installPanel}>
            <code>brew install samba-rgb/wake/wake</code>
            <code>wake -n your-namespace --ui</code>
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
      description="Wake keeps Kubernetes log context intact with terminal filtering, interactive UI, and OpenObserve web viewing.">
      <HomepageHero />
      <main>
        <Workflow />
        <PlatformNotice />
        <QuickStart />
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
