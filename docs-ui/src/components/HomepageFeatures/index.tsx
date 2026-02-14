import type {ReactNode} from 'react';
import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';
import Link from '@docusaurus/Link';

type FeatureItem = {
  title: string;
  icon: string;
  description: ReactNode;
  link: string;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Interactive TUI',
    icon: '🖥️',
    description: (
      <>
        Real-time terminal interface with dynamic filtering and pattern history for efficient log analysis.
      </>
    ),
    link: '/docs/features/interactive-ui',
  },
  {
    title: 'Advanced Patterns',
    icon: '🔍',
    description: (
      <>
        Powerful filtering with logical operators (AND, OR, NOT) and regex support for precise log matching.
      </>
    ),
    link: '/docs/features/advanced-patterns',
  },
  {
    title: 'Web View',
    icon: '🌐',
    description: (
      <>
        Browser-based log viewing with OpenObserve integration for team collaboration and dashboards.
      </>
    ),
    link: '/docs/features/web-view',
  },
  {
    title: 'Template System',
    icon: '🔧',
    description: (
      <>
        Run JFR recordings, heap dumps, and thread dumps across multiple pods with organized output.
      </>
    ),
    link: '/docs/features/template-system',
  },
  {
    title: 'Script Execution',
    icon: '📜',
    description: (
      <>
        Execute custom scripts across multiple pods and collect organized output for debugging tasks.
      </>
    ),
    link: '/docs/features/script-execution',
  },
  {
    title: 'Resource Monitor',
    icon: '📊',
    description: (
      <>
        Real-time CPU and memory monitoring integrated with log streams for performance insights.
      </>
    ),
    link: '/docs/features/monitor',
  },
];

function Feature({title, icon, description, link}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <Link to={link} className={styles.featureLink}>
        <div className={styles.featureCard}>
          <div className={styles.featureIcon}>{icon}</div>
          <div className={styles.featureContent}>
            <Heading as="h3" className={styles.featureTitle}>{title}</Heading>
            <p className={styles.featureDescription}>{description}</p>
          </div>
        </div>
      </Link>
    </div>
  );
}

export default function HomepageFeatures(): ReactNode {
  return (
    <>
      <section className={styles.features}>
        <div className="container">
          <div className={styles.featuresHeader}>
            <Heading as="h2" className={styles.featuresTitle}>
              Essential Features
            </Heading>
            <p className={styles.featuresSubtitle}>
              Everything you need for effective Kubernetes log analysis
            </p>
          </div>
          <div className="row">
            {FeatureList.map((props, idx) => (
              <Feature key={idx} {...props} />
            ))}
          </div>
        </div>
      </section>
      <section className={styles.contactSection}>
        <div className="container">
          <h3 className={styles.contactTitle}>Get in Touch</h3>
          <div className={styles.contactLinks}>
            <a 
              href="https://www.linkedin.com/in/samba-kolliboina/" 
              target="_blank" 
              rel="noopener noreferrer"
              className={styles.contactLink}
            >
              <span className={styles.contactIcon}>💼</span>
              LinkedIn
            </a>
            <a 
              href="mailto:samba24052001@gmail.com" 
              className={styles.contactLink}
            >
              <span className={styles.contactIcon}>📧</span>
              Gmail
            </a>
          </div>
        </div>
      </section>
    </>
  );
}
