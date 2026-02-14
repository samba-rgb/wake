import useBaseUrl from '@docusaurus/useBaseUrl';

export const ConfigTut = () => {
    const configUrl = useBaseUrl('/config.png');
    return (
        <img src={configUrl} alt="Configuration" />
    )
}
