import useBaseUrl from '@docusaurus/useBaseUrl';

export const InteractiveUITut = () => {
    const tuiImg = useBaseUrl('/tui.png');
    return (
        <img src={tuiImg} alt="Interactive TUI" />
    )
}
