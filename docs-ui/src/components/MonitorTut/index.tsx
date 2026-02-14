import useBaseUrl from '@docusaurus/useBaseUrl';

export const MonitorTut = () => {
    const monitorUrl = useBaseUrl('/img/monitor.png');
    return (
        <img src={monitorUrl} alt="Monitor UI" />
    )
}
