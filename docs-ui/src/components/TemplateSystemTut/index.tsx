import useBaseUrl from '@docusaurus/useBaseUrl';

export const TemplateSystemTut = () => {
    const templateUrl = useBaseUrl('/template_running.png');
    return (
        <img src={templateUrl} alt="Template System UI" />
    )
}
