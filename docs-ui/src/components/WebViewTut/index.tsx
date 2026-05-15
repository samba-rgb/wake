import useBaseUrl from '@docusaurus/useBaseUrl';

export const WebViewTut = () => {
    const webUrl = useBaseUrl('/web.png');

    return (
        <figure style={{ margin: '1.5rem 0 2rem' }}>
            <img
                src={webUrl}
                alt="OpenObserve Logs view showing Wake logs in a runtime stream"
                style={{
                    width: '100%',
                    borderRadius: '8px',
                    border: '1px solid var(--ifm-color-emphasis-200)',
                    boxShadow: '0 12px 32px rgba(0, 0, 0, 0.08)',
                }}
            />
            <figcaption
                style={{
                    color: 'var(--ifm-color-emphasis-700)',
                    fontSize: '0.9rem',
                    marginTop: '0.5rem',
                }}
            >
                Wake logs in OpenObserve, grouped by runtime stream.
            </figcaption>
        </figure>
    );
};
