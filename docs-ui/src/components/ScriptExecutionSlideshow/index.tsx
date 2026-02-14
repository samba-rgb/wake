import React from 'react';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

const images = [
  '/img/script_feature/script_list.png',
  '/img/script_feature/script_selector.png',
  '/img/script_feature/script_selection.png',
  '/img/script_feature/script_editor.png',
  '/img/script_feature/argument_input.png',
  '/img/script_feature/execution_progress.png',
  '/img/script_feature/execution_complete.png',
];

export const ScriptExecutionSlideshow = () => {
  const [currentIndex, setCurrentIndex] = React.useState(0);

  const goToPrevious = () => {
    const isFirstSlide = currentIndex === 0;
    const newIndex = isFirstSlide ? images.length - 1 : currentIndex - 1;
    setCurrentIndex(newIndex);
  };

  const goToNext = () => {
    const isLastSlide = currentIndex === images.length - 1;
    const newIndex = isLastSlide ? 0 : currentIndex + 1;
    setCurrentIndex(newIndex);
  };

  const imageUrl = useBaseUrl(images[currentIndex]);

  return (
    <div className={styles.slideshowContainer}>
      <div className={styles.slide} style={{ backgroundImage: `url(${imageUrl})` }}></div>
      <button className={styles.prev} onClick={goToPrevious}>&#10094;</button>
      <button className={styles.next} onClick={goToNext}>&#10095;</button>
    </div>
  );
};
