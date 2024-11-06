## CHANGELOG

### v1.1.0

#### ✨ Enhancements

- Now the scaling factor will be adjusted by EXIF ([a0d05e4237](https://git.bigdick.live/ysun/pineapplehub/commit/a0d05e4237c8eda9e11076776a9e41b19e4b3fed))

### v1.0.1

#### ✨ Enhancements

- Now if user forget press <kbd>RESET</kbd>, it will pop up a dialog to confirm before resetting ([916c1d4d35](https://git.bigdick.live/ysun/pineapplehub/commit/916c1d4d3574d3bc07932895c75c24044de1738a))

### v1.0.0

#### ✨ Enhancements

- The ruler will be ignored now ([#3](https://git.bigdick.live/ysun/pineapplehub/pulls/3))

#### 🚀 Performance improvements

- Continuously resizing the window can be faster now ([#3](https://git.bigdick.live/ysun/pineapplehub/pulls/3))
- Remove unnecessary pre-processing steps ([#3](https://git.bigdick.live/ysun/pineapplehub/pulls/3))

#### 🐞 Bug fixes

- Add more augmentation operation for better scaler recognition ([#3](https://git.bigdick.live/ysun/pineapplehub/pulls/3)):
    - Dirty
    - Too bright or too dark
- Make <kbd>RESET</kbd> button always works ([#3](https://git.bigdick.live/ysun/pineapplehub/pulls/3) and [6fae237bab](https://git.bigdick.live/ysun/pineapplehub/commit/6fae237bab57041b794b7e7464250b56ed0eb15c))
- Fix fitted ecllipse angle ([6fae237bab](https://git.bigdick.live/ysun/pineapplehub/commit/6fae237bab57041b794b7e7464250b56ed0eb15c))

#### 🎨 UI/UX improvements

- Add **CHANGE LOG** drawer
- The details can be hidden now
- The <kbd>COMPUTE</kbd> button will be disabled during calculation