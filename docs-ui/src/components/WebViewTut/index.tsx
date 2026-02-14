import useBaseUrl from '@docusaurus/useBaseUrl';

export const WebViewTut = () => {
    const webUrl = useBaseUrl('/web.png');
    return (
        <img src={webUrl} alt="Web View UI" />
    )
}