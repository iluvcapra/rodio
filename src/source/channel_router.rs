use crate::{Sample, Source};
use std::{cmp::min, sync::Mutex};

pub type ChannelMap = Vec<Vec<f32>>;

/// Internal function that builds a [`ChannelRouter<I>`] object.
pub fn channel_router<I>(input: I, channel_count: u16, channel_map: ChannelMap) -> ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    ChannelRouterSource::new(input, channel_count, channel_map)
}


/// A source for extracting, reordering, mixing and duplicating audio between
/// channels.
#[derive(Debug)]
pub struct ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Input [`Source`]
    input: I,

    /// Mapping of input to output channels
    channel_map: Mutex<ChannelMap>,
    
    /// The output channel that [`next()`] will return next.
    current_channel: u16,

    /// The number of output channels
    channel_count: u16,

    /// The current input audio frame
    input_buffer: Vec<I::Item>,
}

impl<I> ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Creates a new [`ChannelRouter<I>`].
    ///
    /// The new `ChannelRouter` will read samples from `input` and will mix and map them according
    /// to `channel_mappings` into its output samples.
    ///
    /// # Panics
    ///
    /// - if `channel_count` is not equal to `channel_map`'s second dimension
    /// - if `input.channels()` is not equal to `channel_map`'s first dimension
    pub fn new(input: I, channel_count: u16, channel_map: ChannelMap) -> Self {
        assert!(channel_count as usize == channel_map[0].len());
        assert!(input.channels() as usize == channel_map.len());
        Self {
            input,
            channel_map: Mutex::new(channel_map),
            current_channel: channel_count,
            // this will cause the input buffer to fill on first call to next()
            channel_count,
            input_buffer: vec![],
        }
    }

    /// Set or update the gain setting for a channel mapping.
    ///
    /// A channel from the input may be routed to any number of channels in the output, and a
    /// channel in the output may be a mix of any number of channels in the input.
    ///
    /// Successive calls to `mix` with the same `from` and `to` arguments will replace the
    /// previous gain value with the new one.
    pub fn mix(&mut self, from: u16, to: u16, gain: f32) {
        self.channel_map.lock().unwrap()[from as usize][to as usize] = gain;
    }

    /// Destroys this router and returns the underlying source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }

    /// Get mutable access to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
    }
}

impl<I> Source for ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channel_count
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }
}

impl<I> Iterator for ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_channel >= self.channel_count {
            // We've reached the end of the frame, time to grab another one from the input
            let input_channels = self.input.channels() as usize;

            // This might be too fussy, a source should never break a frame in the middle of an
            // audio frame.
            let samples_to_take = min(
                input_channels,
                self.input.current_frame_len().unwrap_or(usize::MAX),
            );

            // fill the input buffer. If the input is exhausted and returning None this will make
            // the input buffer zero length
            self.input_buffer = self.inner_mut().take(samples_to_take).collect();

            self.current_channel = 0;
        }

        // Find the output sample for current_channel
        let retval = self
            .input_buffer
            .iter()
            // if input_buffer is empty, retval will be None
            .enumerate()
            .map(|(input_channel, in_sample)| {
                // the way this works, the input_buffer need not be totally full, the router will
                // work with whatever samples are available and the missing samples will be assumed
                // to be equilibrium.
                let gain = self.channel_map.lock().unwrap()[input_channel][self.current_channel as usize];
                in_sample.amplify(gain)
            })
            .reduce(|a, b| a.saturating_add(b));

        self.current_channel += 1;
        retval
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}