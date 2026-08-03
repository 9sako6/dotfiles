require 'io/console'
require 'time'

module WorkTimer
  class CorruptLogError < StandardError; end
  class LogAccessError < StandardError; end

  class Duration
    FORMAT = /\A(\d+):([0-5]\d):([0-5]\d)\z/

    def self.format(seconds)
      hours, remainder = seconds.floor.divmod(3600)
      minutes, seconds = remainder.divmod(60)
      Kernel.format('%02d:%02d:%02d', hours, minutes, seconds)
    end

    def self.parse(value, path, line_number)
      match = FORMAT.match(value)
      unless match
        raise CorruptLogError, "corrupt log #{path}: invalid duration at line #{line_number}"
      end

      match[1].to_i * 3600 + match[2].to_i * 60 + match[3].to_i
    end
  end

  class Log
    def initialize(path)
      @path = path
    end

    def total_time
      Duration.format(total_seconds)
    end

    def append(started_at, ended_at)
      File.open(@path, 'a') do |file|
        file.puts started_at, ended_at, Duration.format(ended_at - started_at)
      end
    rescue SystemCallError => error
      raise LogAccessError, "cannot append log #{@path}: #{error.message}"
    end

    private

    def total_seconds
      lines = File.readlines(@path, chomp: true)
      unless (lines.length % 3).zero?
        raise CorruptLogError, "corrupt log #{@path}: incomplete record"
      end

      lines.each_slice(3).with_index.sum do |(started_at, ended_at, duration), index|
        first_line = index * 3 + 1
        parse_timestamp(started_at, first_line)
        parse_timestamp(ended_at, first_line + 1)
        Duration.parse(duration, @path, first_line + 2)
      end
    rescue Errno::ENOENT
      0
    rescue CorruptLogError
      raise
    rescue EncodingError => error
      raise CorruptLogError, "corrupt log #{@path}: #{error.message}"
    rescue SystemCallError => error
      raise LogAccessError, "cannot read log #{@path}: #{error.message}"
    end

    def parse_timestamp(value, line_number)
      Time.strptime(value, '%Y-%m-%d %H:%M:%S %z')
    rescue ArgumentError
      raise CorruptLogError, "corrupt log #{@path}: invalid timestamp at line #{line_number}"
    end
  end

  class Session
    def initialize(input: $stdin, output: $stdout)
      @input = input
      @output = output
    end

    def run
      @output.puts "quit : 'q'"
      before_start
      started_at = Time.now
      view_thread = start_view(started_at)
      wait_for_quit
      ended_at = Time.now
      stop_view(view_thread)
      @output.puts
      after_stop(started_at, ended_at)
    ensure
      stop_view(view_thread) if defined?(view_thread)
    end

    private

    def before_start; end

    def after_stop(_started_at, _ended_at); end

    def start_view(started_at)
      render_elapsed(started_at)
      Thread.new do
        loop do
          sleep 1
          render_elapsed(started_at)
        end
      end
    end

    def render_elapsed(started_at)
      @output.print "\r\033[32m#{Duration.format(Time.now - started_at)}\033[0m"
      @output.flush
    end

    def wait_for_quit
      loop do
        character = @input.tty? ? @input.getch : @input.getc
        raise EOFError, "input ended before 'q'" if character.nil?
        break if character == 'q'
      end
    end

    def stop_view(thread)
      return unless thread&.alive?

      thread.kill
      thread.join
    end
  end

  class RecordedSession < Session
    def initialize(path, input: $stdin, output: $stdout)
      super(input: input, output: output)
      @log = Log.new(path)
      @path = path
    end

    private

    def before_start
      @output.puts "total: #{@log.total_time}"
    end

    def after_stop(started_at, ended_at)
      @output.puts "save : #{@path}"
      @log.append(started_at, ended_at)
      @output.puts "total: #{@log.total_time}"
    end
  end
end
